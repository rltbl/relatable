use std::fmt::Display;
use std::{io::Write, path::Path as FilePath};

use anyhow::Result;
use async_std::sync::{Arc, Mutex};
use axum::http::header;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Response, StatusCode},
    response::{Html, IntoResponse, Json, Redirect},
    routing::get,
    Router,
};
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use indexmap::IndexMap;
use rand::rngs::StdRng;
use rand::seq::IteratorRandom as _;
use rand::Rng as _;
use rand::SeedableRng as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
use tabwriter::TabWriter;
use tokio::net::TcpListener;

#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use sqlx::{Column as _, Row as _};

#[cfg(feature = "sqlx")]
use sqlx_core::any::AnyTypeInfoKind;

// ## API Module

#[derive(Debug)]
pub enum RelatableError {
    /// An error in the Relatable configuration:
    ConfigError(String),
    /// An error that occurred while reading or writing to a CSV/TSV:
    // CsvError(csv::Error),
    /// An error involving the data:
    DataError(String),
    /// An error generated by the underlying database:
    // DatabaseError(sqlx::Error),
    /// An error from an unsupported format
    FormatError(String),
    /// An error in the inputs to a function:
    InputError(String),
    /// An error that occurred while reading/writing to stdio:
    IOError(std::io::Error),
    /// An error that occurred while serialising or deserialising to/from JSON:
    // SerdeJsonError(serde_json::Error),
    /// An error that occurred while parsing a regex:
    // RegexError(regex::Error),
    /// An error that occurred because of a user's action
    UserError(String),
}

impl std::fmt::Display for RelatableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for RelatableError {}

#[derive(Debug)]
pub enum DbConnection {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::AnyPool),

    #[cfg(feature = "rusqlite")]
    Rusqlite(Mutex<rusqlite::Connection>),
}

#[derive(Debug)]
pub struct Relatable {
    pub connection: DbConnection,
    pub default_limit: usize,
}

impl Relatable {
    pub async fn default() -> Result<Self> {
        let path = ".relatable/relatable.db";
        // let url = format!("sqlite://{path}");
        // sqlx::any::install_default_drivers();
        // let connection = DbConnection::Sqlx(sqlx::AnyPool::connect(path).await?);
        #[cfg(feature = "rusqlite")]
        let connection = DbConnection::Rusqlite(Mutex::new(rusqlite::Connection::open(path)?));

        Ok(Self {
            connection,
            default_limit: 100,
        })
    }

    pub fn from(&self, table_name: &str) -> Select {
        Select {
            table_name: table_name.to_string(),
            limit: self.default_limit,
            ..Default::default()
        }
    }

    pub async fn fetch_columns(&self, table_name: &str) -> Result<Vec<Column>> {
        // WARN: SQLite only!
        let statement = format!(r#"SELECT * FROM pragma_table_info("{table_name}");"#);
        Ok(query(&self.connection, &statement)
            .await?
            .iter()
            .map(|row| Column {
                name: row.get_string("name"),
            })
            .filter(|c| !c.name.starts_with("_"))
            .collect())
    }

    pub async fn fetch(&self, select: &Select) -> Result<ResultSet> {
        let columns = self.fetch_columns(&select.table_name).await?;
        let statement = select.to_sql()?;
        tracing::debug!("SQL {statement}");
        let json_rows = query(&self.connection, &statement).await?;

        let count = json_rows.len();
        let total = match json_rows.get(0) {
            Some(row) => row
                .content
                .get("_total")
                .and_then(|x| x.as_u64())
                .unwrap_or(0) as usize,
            None => 0,
        };

        let rows: Vec<Row> = json_rows.vec_into();
        Ok(ResultSet {
            range: Range {
                count,
                total,
                start: select.offset + 1,
                end: select.offset + count,
            },
            select: select.clone(),
            table: Table {
                name: select.table_name.clone(),
                editable: false,
            },
            columns,
            rows,
        })
    }

    pub async fn fetch_json_rows(&self, select: &Select) -> Result<Vec<JsonRow>> {
        let statement = select.to_sql()?;
        query(&self.connection, &statement).await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    value: JsonValue,
    text: String,
}

impl From<&JsonValue> for Cell {
    fn from(value: &JsonValue) -> Self {
        Self {
            value: value.clone(),
            text: match value {
                JsonValue::String(value) => value.to_string(),
                value => format!("{value}"),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    id: usize,
    order: usize,
    cells: IndexMap<String, Cell>,
}

// Ignore columns that start with "_"
impl From<JsonRow> for Row {
    fn from(row: JsonRow) -> Self {
        Self {
            id: row
                .content
                .get("_id")
                .and_then(|i| i.as_u64())
                .unwrap_or_default() as usize,
            order: row
                .content
                .get("_order")
                .and_then(|i| i.as_u64())
                .unwrap_or_default() as usize,
            cells: row
                .content
                .iter()
                .filter(|(k, _)| !k.starts_with("_"))
                .map(|(k, v)| (k.clone(), v.into()))
                .collect(),
        }
    }
}

impl From<Row> for Vec<String> {
    fn from(row: Row) -> Self {
        row.to_strings()
    }
}

impl Row {
    fn to_strings(&self) -> Vec<String> {
        self.cells.values().map(|cell| cell.text.clone()).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    name: String,
    // sqltype: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Table {
    name: String,
    editable: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Range {
    count: usize,
    total: usize,
    start: usize,
    end: usize,
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rows {}-{} of {}", self.start, self.end, self.total)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResultSet {
    select: Select,
    range: Range,
    table: Table,
    columns: Vec<Column>,
    rows: Vec<Row>,
}

impl std::fmt::Display for ResultSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tw = TabWriter::new(vec![]);
        tw.write(format!("{}\n", self.range).as_bytes())
            .unwrap_or_default();
        let header = &self
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<String>>();
        tw.write(format!("{}\n", header.join("\t")).as_bytes())
            .unwrap_or_default();
        for row in &self.rows {
            tw.write(format!("{}\n", row.to_strings().join("\t")).as_bytes())
                .unwrap_or_default();
        }
        tw.flush().expect("TabWriter to flush");
        let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
        write!(f, "{written}")
    }
}

// ## SQL module

// From https://stackoverflow.com/a/78372188
pub trait VecInto<D> {
    fn vec_into(self) -> Vec<D>;
}

impl<E, D> VecInto<D> for Vec<E>
where
    D: From<E>,
{
    fn vec_into(self) -> Vec<D> {
        self.into_iter().map(std::convert::Into::into).collect()
    }
}

pub struct JsonRow {
    content: IndexMap<String, JsonValue>,
}

#[cfg(feature = "sqlx")]
impl From<sqlx::any::AnyRow> for JsonRow {
    fn from(row: sqlx::any::AnyRow) -> Self {
        let mut content = IndexMap::new();
        for column in row.columns() {
            let value = match column.type_info().kind() {
                AnyTypeInfoKind::SmallInt | AnyTypeInfoKind::Integer | AnyTypeInfoKind::BigInt => {
                    let value: i32 = row.try_get(column.ordinal()).unwrap_or_default();
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Real | AnyTypeInfoKind::Double => {
                    let value: f64 = row.try_get(column.ordinal()).unwrap_or_default();
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Text => {
                    let value: String = row.try_get(column.ordinal()).unwrap_or_default();
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Bool => {
                    let value: bool = row.try_get(column.ordinal()).unwrap_or_default();
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Null => JsonValue::Null,
                AnyTypeInfoKind::Blob => unimplemented!("SQL blob"),
            };
            content.insert(column.name().into(), value);
        }
        Self { content }
    }
}

impl JsonRow {
    pub fn new() -> Self {
        Self {
            content: IndexMap::new(),
        }
    }
    pub fn get_string(&self, column_name: &str) -> String {
        let value = self.content.get(column_name);
        match value {
            Some(value) => match value {
                JsonValue::Null => "".to_string(),
                JsonValue::Bool(value) => value.to_string(),
                JsonValue::Number(value) => value.to_string(),
                JsonValue::String(value) => value.to_string(),
                JsonValue::Array(value) => format!("{value:?}"),
                JsonValue::Object(value) => format!("{value:?}"),
            },
            None => unimplemented!("missing value"),
        }
    }
    fn to_strings(&self) -> Vec<String> {
        let mut result = vec![];
        for column_name in self.content.keys() {
            result.push(self.get_string(column_name));
        }
        result
    }
    fn to_string_map(&self) -> IndexMap<String, String> {
        let mut result = IndexMap::new();
        for column_name in self.content.keys() {
            result.insert(column_name.clone(), self.get_string(column_name));
        }
        result
    }
    #[cfg(feature = "rusqlite")]
    fn from_rusqlite(column_names: &Vec<&str>, row: &rusqlite::Row) -> Self {
        let mut content = IndexMap::new();
        for column_name in column_names {
            let value = match row.get_ref(*column_name) {
                Ok(value) => match value {
                    rusqlite::types::ValueRef::Null => JsonValue::Null,
                    rusqlite::types::ValueRef::Integer(value) => JsonValue::from(value),
                    rusqlite::types::ValueRef::Real(value) => JsonValue::from(value),
                    rusqlite::types::ValueRef::Text(value)
                    | rusqlite::types::ValueRef::Blob(value) => {
                        let value = std::str::from_utf8(value).unwrap_or_default();
                        JsonValue::from(value)
                    }
                },
                Err(_) => JsonValue::Null,
            };
            content.insert(column_name.to_string(), value);
        }
        Self { content }
    }
}

impl std::fmt::Display for JsonRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_strings().join("\t"))
    }
}

impl std::fmt::Debug for JsonRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string_map())
    }
}

impl From<JsonRow> for Vec<String> {
    fn from(row: JsonRow) -> Self {
        row.to_strings()
    }
}

// Given a connection and a SQL string, return a vector of JsonRows.
// This is intended as a low-level function that abstracts over the SQL engine,
// and whatever result types it returns.
// Since it uses a vector, statements should be limited to a sane number of rows.
pub async fn query(connection: &DbConnection, statement: &str) -> Result<Vec<JsonRow>> {
    match connection {
        #[cfg(feature = "sqlx")]
        DbConnection::Sqlx(pool) => sqlx::query(statement)
            .map(|row| JsonRow::from(row))
            .fetch_all(pool)
            .await
            .map_err(|e| e.into()),
        #[cfg(feature = "rusqlite")]
        DbConnection::Rusqlite(conn) => {
            // The rusqlite::Connection is not thread-safe
            // so we wrap it with a Mutex
            // that we have to lock() within this scope.
            // It might be better to just re-connect?
            let conn = conn.lock().await;
            let stmt = conn.prepare(statement)?;
            let column_names = stmt.column_names();
            let mut stmt = conn.prepare(statement)?;
            let mut rows = stmt.query([])?;
            let mut result = Vec::new();
            while let Some(row) = rows.next()? {
                result.push(JsonRow::from_rusqlite(&column_names, row));
            }
            Ok(result)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Filter {
    Equals { column: String, value: JsonValue },
}

impl Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            Filter::Equals { column, value } => {
                // TODO: This should be factored out.
                let value = match &value {
                    JsonValue::Null => "NULL".to_string(),
                    JsonValue::Bool(value) => value.to_string(),
                    JsonValue::Number(value) => value.to_string(),
                    JsonValue::String(value) => format!("'{value}'"),
                    JsonValue::Array(value) => format!("'{value:?}'"),
                    JsonValue::Object(value) => format!("'{value:?}'"),
                };
                format!(r#""{column}" = {value}"#)
            }
        };
        write!(f, "{result}")
    }
}

pub type QueryParams = IndexMap<String, String>;

pub enum Format {
    Html,
    Json,
    PrettyJson,
    Default,
}

impl TryFrom<&String> for Format {
    fn try_from(path: &String) -> Result<Self> {
        let path = path.to_lowercase();
        let format = if path.ends_with(".pretty.json") {
            Format::PrettyJson
        } else if path.ends_with(".json") {
            Format::Json
        } else if path.ends_with(".html") || path.ends_with(".htm") {
            Format::Html
        } else if path.contains(".") {
            return Err(
                RelatableError::FormatError(format!("Unknown format for path {path}")).into(),
            );
        } else {
            Format::Default
        };
        Ok(format)
    }

    type Error = anyhow::Error;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Select {
    table_name: String,
    limit: usize,
    offset: usize,
    filters: Vec<Filter>,
}

impl Select {
    pub fn from_path_and_query(path: &str, query_params: &QueryParams) -> Self {
        let table_name = path.split(".").next().unwrap_or_default().to_string();
        let limit: usize = query_params
            .get("limit")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or_default();
        let offset: usize = query_params
            .get("offset")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or_default();
        Self {
            table_name,
            limit,
            offset,
            ..Default::default()
        }
    }
    pub fn limit(mut self, limit: &usize) -> Self {
        self.limit = *limit;
        self
    }
    pub fn offset(mut self, offset: &usize) -> Self {
        self.offset = *offset;
        self
    }
    pub fn filters(mut self, filters: &Vec<String>) -> Result<Self> {
        let eq = Regex::new(r"^(\w+)=(\w+)$").unwrap();
        for filter in filters {
            if eq.is_match(&filter) {
                let captures = eq.captures(&filter).unwrap();
                self = self.eq(
                    &captures.get(1).unwrap().as_str(),
                    &captures.get(2).unwrap().as_str(),
                )?;
            } else {
                return Err(RelatableError::ConfigError(format!("invalid filter {filter}")).into());
            }
        }
        Ok(self)
    }
    pub fn eq<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::Equals {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }
    pub fn to_sql(&self) -> Result<String> {
        let mut lines = Vec::new();
        lines.push("SELECT *,".to_string());
        // WARN: The _total count should probably be optional.
        lines.push("  COUNT(1) OVER() AS _total".to_string());
        lines.push(format!(r#"FROM "{}""#, self.table_name));
        for (i, filter) in self.filters.iter().enumerate() {
            let keyword = if i == 0 { "WHERE" } else { "  AND" };
            lines.push(format!("{keyword} {filter}"));
        }
        if self.limit > 0 {
            lines.push(format!("LIMIT {}", self.limit));
        }
        if self.offset > 0 {
            lines.push(format!("OFFSET {}", self.offset));
        }
        Ok(lines.join("\n"))
    }
}

// ### CLI Module

static COLUMN_HELP: &str = "A column name or label";
static ROW_HELP: &str = "A row number";
static TABLE_HELP: &str = "A table name";

#[derive(Parser, Debug)]
#[command(version,
          about = "Relatable (rltbl): Connect your data!",
          long_about = None)]
pub struct Cli {
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
    unimplemented!("print_value");
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
                RelatableError::ConfigError(format!("Database file already exists")).into(),
            );
        }
    }
    let rltbl = Relatable::default().await?;
    let sql = "CREATE TABLE 'table' (
    _id INTEGER UNIQUE,
    _order INTEGER UNIQUE,
    'table' TEXT PRIMARY KEY
)";
    query(&rltbl.connection, sql).await?;
    let sql = "INSERT INTO 'table' VALUES (1, 1000, 'table'), (2, 2000, 'penguin')";
    query(&rltbl.connection, sql).await?;
    let sql = "CREATE TABLE penguin (
    _id INTEGER UNIQUE,
    _order INTEGER UNIQUE,
    study_name TEXT,
    sample_number INTEGER,
    species TEXT,
    island TEXT,
    individual_id TEXT,
    culmen_length REAL,
    body_mass INTEGER
)";
    query(&rltbl.connection, sql).await?;

    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);

    let count = 1000;
    for i in 1..=count {
        let id = i;
        let order = i * 1000;
        let island = islands.iter().choose(&mut rng).unwrap();
        let culmen_length = rng.gen_range(300..500) as f64 / 10.0;
        let body_mass = rng.gen_range(1000..5000);
        let sql = format!(
            "INSERT INTO 'penguin' VALUES (
            {id}, {order},
            'FAKE123', {id},
            'Pygoscelis adeliae', '{island}', 'N{id}',
            {culmen_length}, {body_mass}
        )"
        );
        query(&rltbl.connection, &sql).await?;
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
    Redirect::permanent("table")
}

async fn respond(rltbl: &Relatable, select: &Select, format: &Format) -> Response<Body> {
    let result = match rltbl.fetch(&select).await {
        Ok(result) => result,
        Err(error) => return get_500(&error),
    };

    // format!("get_table:\nPath: {path}, {table_name}, {extension:?}, {format}\nQuery Parameters: {query_params:?}\nResult Set: {pretty}")
    let response = match format {
        Format::Html | Format::Default => {
            Html(format!("<h1>{}</h1><pre>{result}</pre>", result.table.name)).into_response()
        }
        Format::PrettyJson => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (headers, to_string_pretty(&result).unwrap_or_default()).into_response()
        }
        Format::Json => Json(&result).into_response(),
    };
    response
}

async fn get_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    Query(query_params): Query<QueryParams>,
) -> Response<Body> {
    tracing::info!("get_table({rltbl:?}, {path}, {query_params:?})");
    let format = match Format::try_from(&path) {
        Ok(format) => format,
        Err(error) => return get_404(&error),
    };
    let select = Select::from_path_and_query(&path, &query_params);
    respond(&rltbl, &select, &format).await
}

pub fn build_app(shared_state: Arc<Relatable>) -> Router {
    Router::new()
        .route("/", get(get_root))
        .route("/table/*path", get(get_table))
        .with_state(shared_state)
}

#[tokio::main]
pub async fn app(rltbl: Relatable, host: &str, port: &u16) -> Result<String> {
    let shared_state = Arc::new(rltbl);

    let app = build_app(shared_state);

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

#[async_std::main]
async fn main() -> Result<()> {
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
        Command::Serve { host, port } => serve(&cli, host, port).await,
        Command::Demo { force } => build_demo(&cli, force).await,
    }
}
