use rltbl::{
    core::{
        Change, ChangeAction, ChangeSet, Cursor, Format, QueryParams, Relatable, RelatableError,
        ResultSet, Select,
    },
    sql::{query, query_value, VecInto},
};
use std::{io::Write, path::Path as FilePath};

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
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use futures::executor::block_on;
use indexmap::IndexMap;
use minijinja::context;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
use tabwriter::TabWriter;
use tokio::net::TcpListener;
use tower_service::Service;

static COLUMN_HELP: &str = "A column name or label";
static ROW_HELP: &str = "A row number";
static TABLE_HELP: &str = "A table name";
static VALUE_HELP: &str = "A value for a cell";

#[derive(Parser, Debug)]
#[command(version,
          about = "Relatable (rltbl): Connect your data!",
          long_about = None)]
pub struct Cli {
    #[arg(long, default_value="", action = ArgAction::Set)]
    user: String,

    #[command(flatten)]
    verbose: Verbosity,

    // Subcommand:
    #[command(subcommand)]
    pub command: Command,
}

// Note that the subcommands are declared below in the order in which we want them to appear
// in the usage statement that is printed when valve is run with the option `--help`.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Get data from the database
    Get {
        #[command(subcommand)]
        subcommand: GetSubcommand,
    },

    /// Set data in the database
    Set {
        #[command(subcommand)]
        subcommand: SetSubcommand,
    },

    /// Run a Relatable server
    Serve {
        /// Server host address
        #[arg(long, default_value="0.0.0.0", action = ArgAction::Set)]
        host: String,

        /// Server port
        #[arg(long, default_value="0", action = ArgAction::Set)]
        port: u16,
    },

    /// Generate a demonstration database
    Demo {
        /// Output format: text, JSON, TSV
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum GetSubcommand {
    /// Get the column header and rows from a given table.
    Table {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        /// Zero or more filters
        #[arg(value_name = "FILTERS", action = ArgAction::Set)]
        filters: Vec<String>,

        /// Output format: text, JSON, TSV
        #[arg(long, default_value="", action = ArgAction::Set)]
        format: String,

        /// Limit to this many rows
        #[arg(long, default_value="100", action = ArgAction::Set)]
        limit: usize,

        /// Offset by this many rows
        #[arg(long, default_value="0", action = ArgAction::Set)]
        offset: usize,
    },

    /// Get the rows from a given table.
    Rows {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        /// Limit to this many rows
        #[arg(long, default_value="100", action = ArgAction::Set)]
        limit: usize,

        /// Offset by this many rows
        #[arg(long, default_value="0", action = ArgAction::Set)]
        offset: usize,
    },

    /// Get the value of a given column of a given row from a given table.
    Value {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetSubcommand {
    /// Set the value of a given column of a given row from a given table.
    Value {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: String,

        #[arg(value_name = "VALUE", action = ArgAction::Set, help = VALUE_HELP)]
        value: String,
    },
}

/// Given a vector of vectors of strings,
/// print text with "elastic tabstops".
pub fn print_text(rows: &Vec<Vec<String>>) -> Result<()> {
    let mut tw = TabWriter::new(vec![]);
    for row in rows {
        tw.write(format!("{}\n", row.join("\t")).as_bytes())?;
    }
    tw.flush()?;
    let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    print!("{written}");
    Ok(())
}

pub fn print_tsv(rows: Vec<Vec<String>>) -> Result<()> {
    for row in rows {
        println!("{}", row.join("\t"));
    }
    Ok(())
}

// Print a table with its column header.
pub async fn print_table(
    _cli: &Cli,
    table_name: &str,
    filters: &Vec<String>,
    format: &str,
    limit: &usize,
    offset: &usize,
) -> Result<()> {
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl
        .from(table_name)
        .filters(filters)?
        .limit(limit)
        .offset(offset);
    match format.to_lowercase().as_str() {
        "json" => {
            let json = json!(rltbl.fetch(&select).await?);
            print!("{}", to_string_pretty(&json)?);
        }
        "text" | "" => {
            print!("{}", rltbl.fetch(&select).await?.to_string());
        }
        _ => unimplemented!("output format {format}"),
    }

    Ok(())
}

// Print rows of a table, without column header.
pub async fn print_rows(_cli: &Cli, table_name: &str, limit: &usize, offset: &usize) -> Result<()> {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl.from(table_name).limit(limit).offset(offset);
    let rows = rltbl.fetch_json_rows(&select).await?.vec_into();
    print_text(&rows)?;
    Ok(())
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) -> Result<()> {
    tracing::debug!("print_value({cli:?}, {table}, {row}, {column})");
    let rltbl = Relatable::default().await?;
    let statement = format!(r#"SELECT "{column}" FROM "{table}" WHERE _id = ?"#);
    let params = json!([row]);
    if let Some(value) = query_value(&rltbl.connection, &statement, Some(&params)).await? {
        let text = match value {
            JsonValue::String(value) => value.to_string(),
            value => format!("{value}"),
        };
        println!("{text}");
    }
    Ok(())
}

// Get the user from the CLI, RLTBL_USER environment variable,
// or the general environment.
pub fn get_cli_user(cli: &Cli) -> String {
    let mut username = cli.user.clone();
    if username == "" {
        username = std::env::var("RLTBL_USER").unwrap_or_default();
    }
    if username == "" {
        username = whoami::username();
    }
    username
}

pub async fn set_value(
    cli: &Cli,
    table: &str,
    row: usize,
    column: &str,
    value: &str,
) -> Result<()> {
    tracing::debug!("set_value({cli:?}, {table}, {row}, {column}, {value})");
    let rltbl = Relatable::default().await?;
    rltbl
        .set_values(&ChangeSet {
            user: get_cli_user(&cli),
            action: ChangeAction::Do,
            table: table.to_string(),
            description: "Set one value".to_string(),
            changes: vec![Change::Update {
                row,
                column: column.to_string(),
                value: to_value(value).unwrap_or_default(),
            }],
        })
        .await?;
    Ok(())
}

pub async fn build_demo(cli: &Cli, force: &bool) -> Result<()> {
    tracing::debug!("build_demo({cli:?}");
    let path = ".relatable/relatable.db";
    let dir = FilePath::new(path)
        .parent()
        .expect("parent should be defined");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        tracing::info!("Created '{dir:?}' directory");
    }
    let file = FilePath::new(path);
    if file.exists() {
        if *force {
            std::fs::remove_file(&file)?;
            tracing::info!("Removed '{file:?}' file");
        } else {
            print!("File {file:?} already exists. Use --force to overwrite");
            return Err(
                RelatableError::ConfigError("Database file already exists".to_string()).into(),
            );
        }
    }

    // Create and populate the table table
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'table' (
      _id INTEGER UNIQUE,
      _order INTEGER UNIQUE,
      'table' TEXT PRIMARY KEY
    )"#;
    query(&rltbl.connection, sql, None).await?;

    let sql = "INSERT INTO 'table' VALUES (1, 1000, 'table'), (2, 2000, 'penguin')";
    query(&rltbl.connection, sql, None).await?;

    // Create the change and history tables
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'user' (
      'name' TEXT PRIMARY KEY,
      'color' TEXT,
      'cursor' TEXT,
      'datetime' TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Create the change and history tables
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'change' (
      change_id INTEGER PRIMARY KEY,
      'datetime' TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
      'user' TEXT NOT NULL,
      'action' TEXT NOT NULL,
      'table' TEXT NOT NULL,
      'description' TEXT,
      'content' TEXT,
      FOREIGN KEY ("user") REFERENCES user("name")
    )"#;
    query(&rltbl.connection, sql, None).await?;

    let sql = r#"CREATE TABLE 'history' (
      history_id INTEGER PRIMARY KEY,
      change_id INTEGER NOT NULL,
      'table' TEXT NOT NULL,
      'row' INTEGER NOT NULL,
      'before' TEXT,
      'after' TEXT,
      FOREIGN KEY ("change_id") REFERENCES change("change_id"),
      FOREIGN KEY ("table") REFERENCES "table"("table")
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Create the penguin table.
    let sql = r#"CREATE TABLE penguin (
      _id INTEGER UNIQUE,
      _order INTEGER UNIQUE,
      study_name TEXT,
      sample_number INTEGER,
      species TEXT,
      island TEXT,
      individual_id TEXT,
      culmen_length REAL,
      body_mass INTEGER
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Populate the penguin table with random data.
    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);
    let count = 1000;
    for i in 1..=count {
        let id = i;
        let order = i * 1000;
        let island = islands.iter().choose(&mut rng).unwrap();
        let culmen_length = rng.gen_range(300..500) as f64 / 10.0;
        let body_mass = rng.gen_range(1000..5000);
        let sql = r#"INSERT INTO 'penguin'
                     VALUES (?, ?, 'FAKE123', ?, 'Pygoscelis adeliae', ?, ?, ?, ?)"#;
        let params = json!([
            id,
            order,
            id,
            island,
            format!("N{id}"),
            culmen_length,
            body_mass,
        ]);
        query(&rltbl.connection, &sql, Some(&params)).await?;
    }

    Ok(())
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

async fn get_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    Query(query_params): Query<QueryParams>,
    session: Session<SessionNullPool>,
) -> Response<Body> {
    // tracing::info!("get_table({rltbl:?}, {path}, {query_params:?})");
    tracing::info!("get_table([rltbl], {path}, {query_params:?})");
    // tracing::info!("SESSION {:?}", session.get_session_id().inner());
    // tracing::info!("SESSIONS {}", session.count().await);

    let username: String = session.get("username").unwrap_or_default();
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
    // let username: String = session.get("username").unwrap_or_default();
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

async fn post_sign_in(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Form(form): Form<IndexMap<String, String>>,
) -> Response<Body> {
    tracing::info!("post_login({form:?})");
    let username = String::new();
    let username = form.get("username").unwrap_or(&username);
    session.set("username", username);

    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    query(&rltbl.connection, &statement, Some(&params))
        .await
        .expect("Update user");

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

async fn post_sign_out(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Form(form): Form<IndexMap<String, String>>,
) -> Response<Body> {
    tracing::debug!("post_login({form:?})");
    let username = String::new();
    let username = form.get("username").unwrap_or(&username);
    session.set("username", username);

    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    query(&rltbl.connection, &statement, Some(&params))
        .await
        .expect("Update user");

    Html(format!(
        r#"<p>Logged in as {username}</p>
        <form method="post">
        <input name="username" value="{username}"/>
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
    let username: String = session.get("username").unwrap_or_default();
    tracing::info!("post_cursor({cursor:?}, {username})");
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
    let rltbl = Relatable::default().await?;
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
pub async fn serve_cgi() -> Result<()> {
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
        .body(body)
        .unwrap();
    tracing::debug!("REQUEST {request:?}");

    let rltbl = Relatable::default().await?;
    let shared_state = Arc::new(rltbl);
    let mut router = build_app(shared_state).await;
    let response = router.call(request).await;
    tracing::debug!("RESPONSE {response:?}");

    let result = serialize_response(response.unwrap()).await;
    std::io::stdout()
        .write_all(&result)
        .expect("Write to STDOUT");
    Ok(())
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

#[async_std::main]
async fn main() -> Result<()> {
    // Handle a CGI request, instead of normal CLI input.
    match std::env::var_os("GATEWAY_INTERFACE").and_then(|p| Some(p.into_string())) {
        Some(Ok(s)) if s == "CGI/1.1" => {
            return serve_cgi().await;
        }
        _ => (),
    };

    let cli = Cli::parse();

    // Initialize tracing using --verbose flags
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(cli.verbose.tracing_level())
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    tracing::debug!("CLI {cli:?}");

    match &cli.command {
        Command::Get { subcommand } => match subcommand {
            GetSubcommand::Table {
                table,
                filters,
                format,
                limit,
                offset,
            } => print_table(&cli, table, filters, format, limit, offset).await,
            GetSubcommand::Rows {
                table,
                limit,
                offset,
            } => print_rows(&cli, table, limit, offset).await,
            GetSubcommand::Value { table, row, column } => {
                print_value(&cli, table, *row, column).await
            }
        },
        Command::Set { subcommand } => match subcommand {
            SetSubcommand::Value {
                table,
                row,
                column,
                value,
            } => set_value(&cli, table, *row, column, value).await,
        },
        Command::Serve { host, port } => serve(&cli, host, port).await,
        Command::Demo { force } => build_demo(&cli, force).await,
    }
}
