//! # rltbl/relatable
//!
//! This is relatable (rltbl::web).

use crate::{self as rltbl, core::render_values};
use rltbl::{
    cli::Cli,
    core::{ChangeSet, Cursor, Format, QueryParams, Relatable, RelatableError, Row, Select},
    sql::JsonRow,
};
use std::{collections::HashSet, io::Write};

use anyhow::Result;
use async_std::sync::Arc;
use axum::{
    body::Body,
    extract::{Json as ExtractJson, Path, Query, State},
    http::header,
    http::{HeaderMap, Response, StatusCode},
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_session::{Session, SessionConfig, SessionLayer, SessionNullPool, SessionStore};
use indexmap::IndexMap;
use minijinja::context;
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
use tokio::net::TcpListener;
use tower_service::Service;

fn forbid() -> Response<Body> {
    (StatusCode::FORBIDDEN, Html(format!("403 Forbidden"))).into_response()
}

fn get_404(error: &anyhow::Error) -> Response<Body> {
    tracing::error!("404 {error:?}");
    (
        StatusCode::NOT_FOUND,
        Html(format!("404 Not Found: {error}")),
    )
        .into_response()
}

fn get_500(error: &anyhow::Error) -> Response<Body> {
    tracing::error!("500 {error:?}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(format!("500 Internal Server Error: {error}")),
    )
        .into_response()
}

async fn get_root(State(rltbl): State<Arc<Relatable>>) -> impl IntoResponse {
    // tracing::info!("request root");
    let default = "table";
    let table = rltbl
        .connection
        .query_value(
            r#"SELECT "table" FROM "table" ORDER BY _order LIMIT 1"#,
            None,
        )
        .await
        .unwrap_or(Some(json!(default)))
        .unwrap_or(json!(default));
    let table = table.as_str().unwrap_or(default);
    Redirect::permanent(format!("{}/table/{table}", rltbl.root).as_str())
}

async fn main_js() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/javascript".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "public, max-age=60".parse().unwrap());
    (headers, include_str!("resources/main.js"))
}

async fn main_css() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "public, max-age=60".parse().unwrap());
    (headers, include_str!("resources/main.css"))
}

async fn respond(rltbl: &Relatable, format: &Format, content: &JsonValue) -> Response<Body> {
    let response = match format {
        Format::Html | Format::Default => match rltbl.render("table.html", content) {
            Ok(html) => Html(html).into_response(),
            Err(error) => {
                tracing::error!("{error:?}");
                return get_500(&error);
            }
        },
        Format::PrettyJson => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (headers, to_string_pretty(content).unwrap_or_default()).into_response()
        }
        Format::ValueJson | Format::Json => Json(content).into_response(),
    };
    response
}

fn get_username(session: Session<SessionNullPool>) -> String {
    let username = std::env::var("RLTBL_USER").unwrap_or_default();
    if username != "" {
        return username;
    }
    session.get("username").unwrap_or_default()
}

async fn get_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    Query(query_params): Query<QueryParams>,
    session: Session<SessionNullPool>,
) -> Response<Body> {
    // tracing::info!("get_table({rltbl:?}, {path}, {query_params:?})");
    // tracing::info!("get_table([rltbl], {path}, {query_params:?})");
    // tracing::info!("SESSION {:?}", session.get_session_id().inner());
    // tracing::info!("SESSIONS {}", session.count().await);

    let username = get_username(session);
    if username.trim() != "" {
        init_user(&rltbl, &username).await;
    }
    // tracing::info!("USERNAME {username}");
    let select = Select::from_path_and_query(&rltbl, &path, &query_params);
    let format = match Format::try_from(&path) {
        Ok(format) => format,
        Err(error) => return get_404(&error),
    };
    let result = match rltbl.fetch(&select).await {
        Ok(result) => result,
        Err(error) => return get_500(&error),
    };
    let site = rltbl.get_site(&username).await;
    let content = json!({"site": site, "path": "table", "result": result});
    respond(&rltbl, &format, &content).await
}

async fn get_tableset(
    State(rltbl): State<Arc<Relatable>>,
    Path((tableset_name, path)): Path<(String, String)>,
    Query(query_params): Query<QueryParams>,
    session: Session<SessionNullPool>,
) -> Response<Body> {
    // tracing::info!("get_tableset({rltbl:?}, {tableset_name}, {path}, {query_params:?})");
    let format = match Format::try_from(&path) {
        Ok(format) => format,
        Err(error) => return get_404(&error),
    };

    let select = Select::from_path_and_query(&rltbl, &path, &query_params);
    // tracing::info!("SELECT {select:?}",);

    if matches!(format, Format::ValueJson) {
        let sel = match joined_query(&rltbl, &tableset_name, &select).await {
            Ok(select) => select,
            Err(error) => return get_500(&error),
        };
        let value = match rltbl.count(&sel).await {
            Ok(count) => count,
            Err(error) => return get_500(&error),
        };
        return Json(value).into_response();
    }

    let username = get_username(session);
    if username.trim() != "" {
        init_user(&rltbl, &username).await;
    }
    // tracing::info!("USERNAME {username}");

    let site = rltbl.get_site(&username).await;

    let sql = format!(r#"SELECT * FROM "tableset" WHERE tableset = '{tableset_name}'"#);
    let json_rows = match rltbl.connection.query(&sql, None).await {
        Ok(rows) => rows,
        Err(error) => return get_500(&error),
    };

    // tracing::info!("TAB {json_rows:?}");
    let mut tabs = vec![];
    for json_row in json_rows {
        let table = json_row.get_string("table").unwrap();
        let mut s = select.clone();
        s.table_name = table.clone();
        let mut c = s.clone();
        c.select = vec!["count()".to_string()];
        tabs.push(json!({
            "table": table,
            "active": (table == select.table_name),
            "disabled": (table == "total"),
            "url": s.to_url(format!("{}/tableset/{tableset_name}", site.root).as_str(), &Format::Default).unwrap(),
            "count": c.to_url(format!("{}/tableset/{tableset_name}", site.root).as_str(), &Format::ValueJson).unwrap()
        }));
    }

    let mut result = match joined_query(&rltbl, &tableset_name, &select).await {
        Ok(sel) => match rltbl.fetch(&sel).await {
            Ok(result) => result,
            Err(error) => return get_500(&error),
        },
        Err(error) => return get_500(&error),
    };
    result.select = select.clone();

    let content = json!({"site": site, "path": format!("tableset/{}", tableset_name), "tabs": tabs, "result": result});
    respond(&rltbl, &format, &content).await
}

async fn joined_query(rltbl: &Relatable, tableset_name: &str, select: &Select) -> Result<Select> {
    let mut tables = HashSet::new();
    tables.insert(json!(select.table_name));
    for filter in &select.filters {
        let (t, _, _, _) = filter.parts();
        if t != "" {
            tables.insert(json!(t));
        }
    }

    if tables.len() == 1 {
        return Ok(select.clone());
    }

    let tables: Vec<JsonValue> = tables.into_iter().collect();
    let values = render_values(&tables).unwrap();

    let sql = format!(
        r#"WITH RECURSIVE ancestors("table", "using") AS (
      SELECT "table", "using"
      FROM tableset
      WHERE "table" IN {values}
      UNION
      SELECT tableset."table", tableset."using"
      FROM ancestors
      JOIN tableset ON ancestors."using" = tableset."distinct"
      WHERE tableset.tableset = '{tableset_name}'
    )
    SELECT tableset.*
    FROM tableset
    JOIN ancestors USING ("table")
    WHERE _order >= (SELECT MIN(_order) FROM tableset WHERE "table" IN {values})
      AND _order <= (SELECT MAX(_order) FROM tableset WHERE "table" IN {values})
    ORDER BY _order"#
    );
    let json_rows = rltbl.connection.query(&sql, None).await?;
    tracing::info!("TABLESET {json_rows:?}",);

    // Build a custom query, something like this:
    // SELECT *
    // FROM hcckb.arm
    // WHERE arm_id IN (
    //   SELECT arm_id
    //   FROM hcckb.arm
    //   JOIN hcckb.participant_2_arm USING (arm_id)
    //   JOIN hcckb.participant USING (participant_id)
    //   LEFT JOIN hcckb.specimen USING (participant_id)
    //   WHERE biological_sex = 'Male'
    //     AND specimen_type = 'PBMC'
    // )
    let limit = select.limit;
    let mut sel = select.clone();
    let table_name = select.table_name.clone();
    let mut pkey = String::new();
    for json_row in json_rows.clone() {
        if table_name == json_row.get_string("table").unwrap() {
            pkey = json_row.get_string("distinct").unwrap();
        }
    }
    sel.select = vec![pkey.clone()];
    if table_name == json_rows.last().unwrap().get_string("table").unwrap() {
        sel.order_by(&pkey);
    } else {
        sel.limit = 0;
    }
    let json_row = json_rows.first().unwrap();
    sel.table_name = json_row.get_string("table").unwrap();
    for json_row in json_rows.iter().skip(1) {
        sel.left_join_using(
            &json_row.get_string("table").unwrap(),
            &json_row.get_string("using").unwrap(),
        );
    }
    Ok(crate::core::Select {
        table_name,
        filters: vec![crate::core::Filter::InSubquery {
            table: String::new(),
            column: pkey.clone(),
            subquery: sel.clone(),
        }],
        limit,
        ..Default::default()
    })
}

async fn post_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    _session: Session<SessionNullPool>,
    ExtractJson(changeset): ExtractJson<ChangeSet>,
) -> Response<Body> {
    tracing::info!("post_table([rltbl], {path}, {changeset:?})");
    if rltbl.readonly {
        return forbid().into();
    }

    let table = changeset.table.clone();
    if path != table {
        return get_500(
            &RelatableError::InputError(format!(
                "Changeset table '{table}' does not match URL path {path}"
            ))
            .into(),
        );
    }

    // WARN: We need to check that the user matches!
    // let user = changeset.user.clone();
    // let username = get_username(session);
    // if username != user {
    //     return get_500(
    //         &RelatableError::InputError(format!(
    //             "Changeset user '{user}' does not match session username {username}"
    //         ))
    //         .into(),
    //     );
    // }

    match rltbl.set_values(&changeset).await {
        Ok(_) => "POST successful".into_response(),
        Err(error) => get_500(&error),
    }
}

async fn init_user(rltbl: &Relatable, username: &str) -> () {
    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    rltbl
        .connection
        .query(&statement, Some(&params))
        .await
        .expect("Update user");
}

async fn post_sign_in(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Form(form): Form<IndexMap<String, String>>,
) -> Response<Body> {
    tracing::info!("post_login({form:?})");
    let username = String::new();
    let username = form.get("username").unwrap_or(&username);
    session.set("username", username);
    init_user(&rltbl, &username).await;

    match form.get("redirect") {
        Some(url) => Redirect::to(url).into_response(),
        None => Html(format!(
            r#"<p>Logged in as {username}</p>
            <form method="post">
            <input name="username" value="{username}"/>
            <input type="submit"/>
            </form>"#
        ))
        .into_response(),
    }
}

async fn post_sign_out(session: Session<SessionNullPool>) -> Response<Body> {
    tracing::debug!("post_logout()");
    session.set("username", "");

    Html(format!(
        r#"<p>Logged out</p>
        <form method="post">
        <input name="username" value=""/>
        <input type="submit"/>
        </form>"#
    ))
    .into_response()
}

async fn post_cursor(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    ExtractJson(cursor): ExtractJson<Cursor>,
) -> Response<Body> {
    // tracing::info!("post_cursor({cursor:?})");
    let username = get_username(session);
    tracing::debug!("post_cursor({cursor:?}, {username})");
    // TODO: sanitize the cursor JSON.
    let statement = format!(
        r#"UPDATE user
           SET "cursor" = ?,
               "datetime" = CURRENT_TIMESTAMP
           WHERE "name" = ?"#,
    );
    let cursor = to_value(cursor).unwrap_or_default();
    let params = json!([cursor, username]);
    match rltbl.connection.query(&statement, Some(&params)).await {
        Ok(_) => "Cursor updated".into_response(),
        Err(_) => "Cursor update failed".into_response(),
    }
}

async fn get_row_menu(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table_name, row_id)): Path<(String, usize)>,
) -> Response<Body> {
    // tracing::info!("get_row_menu({table_name}, {row_id})");
    let username = get_username(session);
    let site = rltbl.get_site(&username).await;
    let table = match rltbl.get_table(&table_name).await {
        Ok(table) => table,
        Err(error) => return get_404(&error),
    };
    let row: Row = match rltbl
        .connection
        .query_one(
            &format!(r#"SELECT * FROM "{}" WHERE _id = $1"#, table.view),
            Some(&json!([row_id])),
        )
        .await
    {
        Ok(row) => match row {
            Some(row) => row.into(),
            None => {
                return get_404(
                    &RelatableError::MissingError(format!(
                        "No row in '{table_name}' with id {row_id}"
                    ))
                    .into(),
                )
            }
        },
        Err(error) => return get_500(&error),
    };
    match rltbl.render("row_menu.html", context! {site, table, row}) {
        Ok(html) => Html(html).into_response(),
        Err(error) => return get_500(&error),
    }
}

async fn get_column_menu(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table_name, column)): Path<(String, String)>,
    Query(query_params): Query<QueryParams>,
) -> Response<Body> {
    // tracing::info!("get_column_menu({table_name}, {column})");
    let username = get_username(session);
    let select = Select::from_path_and_query(&rltbl, &table_name, &query_params);
    let mut operator = String::new();
    let mut value = json!("");
    let mut order = String::new();
    for filter in select.filters {
        let (_, c, o, v) = filter.parts();
        tracing::warn!("FILTER {filter:?} {o}");
        if c == column {
            operator = o;
            value = v;
        }
    }
    for (c, o) in select.order_by {
        if c == column {
            order = format!("{o:?}");
        }
    }
    let site = rltbl.get_site(&username).await;
    match rltbl.render(
        "column_menu.html",
        context! {site, table_name, column, operator, value, order},
    ) {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}
async fn get_cell_menu(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table_name, row_id, column)): Path<(String, usize, String)>,
) -> Response<Body> {
    // tracing::info!("get_cell_menu({table_name}, {row_id}, {column})");
    let username = get_username(session);
    let site = rltbl.get_site(&username).await;
    let table = match rltbl.get_table(&table_name).await {
        Ok(table) => table,
        Err(error) => return get_404(&error),
    };
    let row: Row = match rltbl
        .connection
        .query_one(
            &format!(r#"SELECT * FROM "{}" WHERE _id = $1"#, table.view),
            Some(&json!([row_id])),
        )
        .await
    {
        Ok(row) => match row {
            Some(row) => row.into(),
            None => {
                return get_404(
                    &RelatableError::MissingError(format!(
                        "No row in '{table_name}' with id {row_id}"
                    ))
                    .into(),
                )
            }
        },
        Err(error) => return get_500(&error),
    };
    let cell = row.cells.get(&column);
    match rltbl.render("cell_menu.html", context! {site, table, row, column, cell}) {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}

async fn get_cell_options(
    State(rltbl): State<Arc<Relatable>>,
    Path((table, _row_id, column)): Path<(String, usize, String)>,
    Query(query_params): Query<QueryParams>,
) -> Response<Body> {
    // tracing::info!("get_cell_option({table}, {row_id}, {column}, {query_params:?})");
    let input = match query_params.get("input") {
        Some(input) => input,
        None => &String::new(),
    };
    let statement = format!(
        r#"SELECT DISTINCT "{column}" AS "value" FROM "{table}"
           WHERE "{column}" LIKE '%{input}%' AND "{column}" != ''
           LIMIT 20"#
    );
    let values: Vec<JsonValue> = rltbl
        .connection
        .query(&statement, None)
        .await
        .expect("Get column values")
        .iter()
        .map(|row| {
            let value = row.get_string("value").expect("No 'value' in row");
            json!({
                    "value": value,
                    "label": value,
            })
        })
        .collect();
    Json(json!(values)).into_response()
}

async fn previous_row_id(rltbl: &Relatable, table: &str, row_id: &usize) -> usize {
    let sql = format!(
        r#"SELECT "_id", MAX("_order") FROM "{table}"
        WHERE "_order" < (SELECT "_order" FROM "{table}" WHERE _id = ?)"#
    );
    let after_id = rltbl
        .connection
        .query_value(&sql, Some(&json!([row_id])))
        .await;
    after_id
        .unwrap_or(Some(json!(0)))
        .unwrap_or(json!(0))
        .as_u64()
        .unwrap_or_default() as usize
}

async fn add_row_before(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table, row_id)): Path<(String, usize)>,
) -> Response<Body> {
    tracing::info!("add_row_before({table}, {row_id})");
    let username = get_username(session);
    let after_id = previous_row_id(&rltbl, &table, &row_id).await;
    return add_row(&rltbl, &username, &table, Some(after_id)).await;
}

async fn add_row_after(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table, row_id)): Path<(String, usize)>,
) -> Response<Body> {
    tracing::info!("add_row_after({table}, {row_id})");
    let username = get_username(session);
    return add_row(&rltbl, &username, &table, Some(row_id)).await;
}

async fn add_row_end(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path(table): Path<String>,
) -> Response<Body> {
    tracing::info!("add_row_end({table})");
    let username = get_username(session);
    return add_row(&rltbl, &username, &table, None).await;
}

async fn add_row(
    rltbl: &Relatable,
    username: &str,
    table: &str,
    after_id: Option<usize>,
) -> Response<Body> {
    if rltbl.readonly {
        return forbid().into();
    }
    let columns = match rltbl.fetch_columns(&table).await {
        Ok(columns) => columns,
        Err(error) => return get_500(&error),
    };
    let json_row: JsonRow = JsonRow {
        content: columns
            .iter()
            .map(|c| (c.name.clone(), json!(String::new())))
            .collect(),
    };
    match rltbl.add_row(&table, &username, after_id, &json_row).await {
        Ok(row) => {
            // tracing::info!("Added row {row:?}");
            let offset = rltbl
                .connection
                .query_value(
                    &format!(r#"SELECT COUNT(*) FROM "{table}" WHERE _order <= ?"#),
                    Some(&json!([row.order])),
                )
                .await;
            let offset: u64 = offset
                .unwrap_or(Some(json!(0)))
                .unwrap_or(json!(0))
                .as_u64()
                .unwrap_or_default();
            let url = format!("{}/table/{table}?offset={offset}", rltbl.root);
            return Redirect::temporary(url.as_str()).into_response();
        }
        Err(error) => return get_500(&error),
    }
}

async fn delete_row(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Path((table, row_id)): Path<(String, usize)>,
) -> Response<Body> {
    tracing::info!("delete_row({table}, {row_id})");
    if rltbl.readonly {
        return forbid().into();
    }

    let username = get_username(session);
    let prev = previous_row_id(&rltbl, &table, &row_id).await;
    match rltbl.delete_row(&table, &username, row_id).await {
        Ok(_) => {
            let offset = rltbl
                .connection
                .query_value(
                    &format!(
                        r#"SELECT COUNT(*) FROM "{table}"
                       WHERE _order <= (SELECT _order FROM "{table}" WHERE _id = ?)"#
                    ),
                    Some(&json!([prev])),
                )
                .await;
            let offset: u64 = offset
                .unwrap_or(Some(json!(0)))
                .unwrap_or(json!(0))
                .as_u64()
                .unwrap_or_default();
            let url = format!("{}/table/{table}?offset={offset}", rltbl.root);
            Redirect::temporary(url.as_str()).into_response()
        }
        Err(error) => return get_500(&error),
    }
}

pub async fn build_app(shared_state: Arc<Relatable>) -> Router {
    let session_config = SessionConfig::default();
    let session_store = SessionStore::<SessionNullPool>::new(None, session_config)
        .await
        .unwrap();
    Router::new()
        .route("/", get(get_root))
        .route("/static/main.js", get(main_js))
        .route("/static/main.css", get(main_css))
        .route("/sign-in", post(post_sign_in))
        .route("/sign-out", post(post_sign_out))
        .route("/cursor", post(post_cursor))
        .route("/table/{*path}", get(get_table).post(post_table))
        .route("/tableset/{tableset_name}/{*path}", get(get_tableset))
        .route("/row-menu/{table_name}/{row_id}", get(get_row_menu))
        .route("/column-menu/{table_name}/{column}", get(get_column_menu))
        .route(
            "/cell-menu/{table_name}/{row_id}/{column}",
            get(get_cell_menu),
        )
        .route(
            "/cell-options/{table}/{row_id}/{column}",
            get(get_cell_options),
        )
        .route("/add-row/{table}", get(add_row_end))
        .route("/add-row-before/{table}/{row_id}", get(add_row_before))
        .route("/add-row-after/{table}/{row_id}", get(add_row_after))
        .route("/delete-row/{table}/{row_id}", get(delete_row))
        .layer(SessionLayer::new(session_store))
        .with_state(shared_state)
}

#[tokio::main]
pub async fn app(rltbl: Relatable, host: &str, port: &u16, timeout: &usize) -> Result<String> {
    let shared_state = Arc::new(rltbl);

    let app = build_app(shared_state).await;

    // Create a `TcpListener` using tokio.
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await.expect("valid TCP address");
    println!(
        "Running Relatable server at http://{}",
        listener.local_addr()?
    );

    // Run the server with graceful shutdown
    if *timeout == 0 {
        println!("Press Control-C to quit.");
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_on_signal())
            .await
            .unwrap();
    } else {
        println!("Running server for {timeout}s.");
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_on_timeout(*timeout))
            .await
            .unwrap();
    }

    Ok("Stopping Relatable server...".into())
}

pub async fn serve(_cli: &Cli, host: &str, port: &u16, timeout: &usize) -> Result<()> {
    tracing::debug!("serve({host}, {port})");
    let rltbl = Relatable::connect(None).await?;
    app(rltbl, host, port, timeout)?;
    Ok(())
}

async fn shutdown_on_timeout(timeout: usize) {
    std::thread::sleep(std::time::Duration::from_secs(timeout as u64));

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let _ = terminate;
}

// From https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs
async fn shutdown_on_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

// Read CGI variables from the environment,
// and read STDIN in the case of POST,
// then handle the request,
// and send the HTTP response to STDOUT.
pub async fn serve_cgi() {
    let request_method = std::env::var("REQUEST_METHOD").unwrap_or("GET".to_string());
    let path_info = std::env::var("PATH_INFO").unwrap_or("/".to_string());
    let query_string = std::env::var("QUERY_STRING").unwrap_or_default();
    let query_string = query_string.trim();
    let uri = if query_string == "" {
        path_info
    } else {
        format!("{path_info}?{query_string}")
    };
    let body = if request_method == "POST" {
        std::io::stdin()
            .lines()
            .map(|l| l.unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };

    let request = axum::http::Request::builder()
        .method(request_method.as_str())
        .uri(uri)
        .header("Accept", std::env::var("HTTP_ACCEPT").unwrap_or_default())
        .header(
            "Content-Type",
            std::env::var("CONTENT_TYPE").unwrap_or_default(),
        )
        .header(
            "Content-Length",
            std::env::var("CONTENT_LENGTH").unwrap_or_default(),
        )
        .body(body)
        .unwrap();
    tracing::debug!("REQUEST {request:?}");

    let rltbl = Relatable::connect(None).await.expect("Database connection");
    let shared_state = Arc::new(rltbl);
    let mut router = build_app(shared_state).await;
    let response = router.call(request).await;
    tracing::debug!("RESPONSE {response:?}");

    let result = serialize_response(response.unwrap()).await;
    std::io::stdout()
        .write_all(&result)
        .expect("Write to STDOUT");
}

// From https://github.com/amandasaurus/rust-cgi/blob/main/src/lib.rs
// Turn a Response into an HTTP response as bytes.
async fn serialize_response(response: Response<Body>) -> Vec<u8> {
    let mut output = String::new();
    output.push_str("Status: ");
    output.push_str(response.status().as_str());
    if let Some(reason) = response.status().canonical_reason() {
        output.push_str(" ");
        output.push_str(reason);
    }
    output.push_str("\n");

    {
        let headers = response.headers();
        let mut keys: Vec<&http::header::HeaderName> = headers.keys().collect();
        keys.sort_by_key(|h| h.as_str());
        for key in keys {
            output.push_str(key.as_str());
            output.push_str(": ");
            output.push_str(headers.get(key).unwrap().to_str().unwrap());
            output.push_str("\n");
        }
    }

    output.push_str("\n");

    let mut output = output.into_bytes();

    let (_, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("Read from response body");
    output.append(&mut bytes.to_vec());

    output
}
