//! # rltbl/relatable
//!
//! This is relatable (rltbl::web).

use crate::{
    cli::Cli,
    core::{ChangeSet, Cursor, Format, QueryParams, Relatable, RelatableError, ResultSet, Select},
    sql::query,
};
use std::io::Write;

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
use futures::executor::block_on;
use indexmap::IndexMap;
use minijinja::context;
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
use tokio::net::TcpListener;
use tower_service::Service;

fn forbid() -> Response<Body> {
    (StatusCode::FORBIDDEN, Html(format!("403 Forbidden"))).into_response()
}

fn get_404(error: &anyhow::Error) -> Response<Body> {
    (
        StatusCode::NOT_FOUND,
        Html(format!("404 Not Found: {error}")),
    )
        .into_response()
}

fn get_500(error: &anyhow::Error) -> Response<Body> {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(format!("500 Internal Server Error: {error}")),
    )
        .into_response()
}

async fn get_root() -> impl IntoResponse {
    tracing::info!("request root");
    Redirect::permanent("/table/table")
}

async fn main_js() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/javascript".parse().unwrap());
    (headers, include_str!("resources/main.js"))
}

async fn main_css() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
    (headers, include_str!("resources/main.css"))
}

async fn render_html(rltbl: &Relatable, username: &str, result: &ResultSet) -> Result<String> {
    let site = rltbl.get_site(username).await;
    rltbl.render("table.html", context! {site, result})
}

async fn render_response(rltbl: &Relatable, username: &str, result: &ResultSet) -> Response<Body> {
    match render_html(rltbl, username, result).await {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}

async fn respond(
    rltbl: &Relatable,
    username: &str,
    select: &Select,
    format: &Format,
) -> Response<Body> {
    let result = match rltbl.fetch(&select).await {
        Ok(result) => result,
        Err(error) => return get_500(&error),
    };

    // format!(
    //     "get_table:\nPath: {path}, {table_name}, {extension:?}, {format}\nQuery Parameters: \
    //      {query_params:?}\nResult Set: {pretty}"
    // );
    let response = match format {
        Format::Html | Format::Default => render_response(&rltbl, &username, &result).await,
        Format::PrettyJson => {
            let site = rltbl.get_site(username).await;
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (
                headers,
                to_string_pretty(&json!({"site": site, "result": result})).unwrap_or_default(),
            )
                .into_response()
        }
        Format::Json => {
            let site = rltbl.get_site(username).await;
            Json(&json!({"site": site, "result": result})).into_response()
        }
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
    tracing::info!("USERNAME {username}");
    let select = Select::from_path_and_query(&rltbl, &path, &query_params);
    let format = match Format::try_from(&path) {
        Ok(format) => format,
        Err(error) => return get_404(&error),
    };
    respond(&rltbl, &username, &select, &format).await
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

    // Axum is complaining when we replace the call to block_on() here with
    // .await. Why?
    match block_on(rltbl.set_values(&changeset)) {
        Ok(_) => "POST successful".into_response(),
        Err(error) => get_500(&error),
    }
}

async fn init_user(rltbl: &Relatable, username: &str) -> () {
    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    query(&rltbl.connection, &statement, Some(&params))
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
    match query(&rltbl.connection, &statement, Some(&params)).await {
        Ok(_) => "Cursor updated".into_response(),
        Err(_) => "Cursor update failed".into_response(),
    }
}

async fn get_row_menu(
    State(rltbl): State<Arc<Relatable>>,
    Path((table, row)): Path<(String, usize)>,
) -> Response<Body> {
    tracing::info!("get_row_menu({table}, {row})");
    match rltbl.render("row_menu.html", context! {table, row}) {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}
async fn get_column_menu(
    State(rltbl): State<Arc<Relatable>>,
    Path((table, column)): Path<(String, String)>,
) -> Response<Body> {
    tracing::info!("get_column_meny({table}, {column})");
    match rltbl.render("column_menu.html", context! {table, column}) {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}
async fn get_cell_menu(
    State(rltbl): State<Arc<Relatable>>,
    Path((table, row, column)): Path<(String, usize, String)>,
) -> Response<Body> {
    tracing::info!("get_cell_menu({table}, {row}, {column})");
    match rltbl.render("cell_menu.html", context! {table, row, column}) {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}

async fn get_cell_options(
    State(rltbl): State<Arc<Relatable>>,
    Path((table, row, column)): Path<(String, usize, String)>,
    Query(query_params): Query<QueryParams>,
) -> Response<Body> {
    tracing::info!("get_cell_option({table}, {row}, {column}, {query_params:?})");
    let input = match query_params.get("input") {
        Some(input) => input,
        None => &String::new(),
    };
    let statement = format!(
        r#"SELECT DISTINCT "{column}" AS 'value' FROM "{table}" WHERE "{column}" LIKE '%{input}%' LIMIT 20"#
    );
    let values: Vec<JsonValue> = query(&rltbl.connection, &statement, None)
        .await
        .expect("Get column values")
        .iter()
        .map(|row| {
            let value = row.get_string("value");
            json!({
                    "value": value,
                    "label": value,
            })
        })
        .collect();
    Json(json!(values)).into_response()
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
        .route("/row-menu/{table}/{row}", get(get_row_menu))
        .route("/column-menu/{table}/{column}", get(get_column_menu))
        .route("/cell-menu/{table}/{row}/{column}", get(get_cell_menu))
        .route(
            "/cell-options/{table}/{row}/{column}",
            get(get_cell_options),
        )
        .layer(SessionLayer::new(session_store))
        .with_state(shared_state)
}

#[tokio::main]
pub async fn app(rltbl: Relatable, host: &str, port: &u16) -> Result<String> {
    let shared_state = Arc::new(rltbl);

    let app = build_app(shared_state).await;

    // Create a `TcpListener` using tokio.
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await.expect("valid TCP address");
    println!(
        "Running Relatable server at http://{}",
        listener.local_addr()?
    );
    println!("Press Control-C to quit.");

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    Ok("Stopping Relatable server...".into())
}

pub async fn serve(_cli: &Cli, host: &str, port: &u16) -> Result<()> {
    tracing::debug!("serve({host}, {port})");
    let rltbl = Relatable::connect().await?;
    app(rltbl, host, port)?;
    Ok(())
}

// From https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs
async fn shutdown_signal() {
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

    let rltbl = Relatable::connect().await.expect("Database connection");
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
