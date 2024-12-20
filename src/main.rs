use std::io::Write;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use tabwriter::TabWriter;

#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use sqlx::{Column as _, Row as _};

#[cfg(feature = "sqlx")]
use sqlx_core::any::AnyTypeInfoKind;

// ## API Module

#[derive(Debug)]
pub enum RelatableError {
    /// An error in the Valve configuration:
    ConfigError(String),
    /// An error that occurred while reading or writing to a CSV/TSV:
    // CsvError(csv::Error),
    /// An error involving the data:
    DataError(String),
    /// An error generated by the underlying database:
    // DatabaseError(sqlx::Error),
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

pub enum DbConnection {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::AnyPool),

    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Connection),
}

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
        let connection = DbConnection::Rusqlite(rusqlite::Connection::open(path)?);

        Ok(Self {
            connection,
            default_limit: 100,
        })
    }

    pub fn from(&self, table_name: &str) -> Select {
        Select {
            table_name: table_name.to_string(),
            limit: self.default_limit,
            offset: 0,
        }
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
                    json!(value)
                }
                AnyTypeInfoKind::Real | AnyTypeInfoKind::Double => {
                    let value: f64 = row.try_get(column.ordinal()).unwrap_or_default();
                    json!(value)
                }
                AnyTypeInfoKind::Text => {
                    let value: String = row.try_get(column.ordinal()).unwrap_or_default();
                    JsonValue::String(value)
                }
                AnyTypeInfoKind::Bool => {
                    let value: bool = row.try_get(column.ordinal()).unwrap_or_default();
                    json!(value)
                }
                AnyTypeInfoKind::Null => JsonValue::Null,
                AnyTypeInfoKind::Blob => unimplemented!("SQL blob types are not implemented"),
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
                JsonValue::Array(_) => unimplemented!(),
                JsonValue::Object(_) => unimplemented!(),
            },
            None => unimplemented!(),
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
            let text: String = row.get(*column_name).unwrap_or_default();
            let value: JsonValue = row.get(*column_name).unwrap_or(JsonValue::String(text));
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

#[derive(Debug)]
pub struct DbColumn {
    name: String,
    // sqltype: String,
}

#[derive(Debug)]
pub struct Select {
    table_name: String,
    limit: usize,
    offset: usize,
}

impl Select {
    pub fn limit(mut self, limit: &usize) -> Self {
        self.limit = *limit;
        self
    }
    pub fn offset(mut self, offset: &usize) -> Self {
        self.offset = *offset;
        self
    }
    pub fn to_sql(&self) -> Result<String> {
        let mut sql = vec![];
        sql.push("SELECT *".to_string());
        sql.push(format!(r#"FROM "{}""#, self.table_name));
        if self.limit > 0 {
            sql.push(format!("LIMIT {}", self.limit));
        }
        if self.offset > 0 {
            sql.push(format!("OFFSET {}", self.offset));
        }
        Ok(sql.join("\n"))
    }

    pub async fn query(&self, statement: &str, connection: &DbConnection) -> Result<Vec<JsonRow>> {
        match connection {
            #[cfg(feature = "sqlx")]
            DbConnection::Sqlx(pool) => sqlx::query(statement)
                .map(|row| JsonRow::from(row))
                .fetch_all(pool)
                .await
                .map_err(|e| e.into()),
            #[cfg(feature = "rusqlite")]
            DbConnection::Rusqlite(conn) => {
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

    pub async fn fetch_all(&self, connection: &DbConnection) -> Result<Vec<JsonRow>> {
        let statement = self.to_sql()?;
        self.query(statement.as_str(), connection).await
    }

    pub async fn fetch_columns(&self, connection: &DbConnection) -> Result<Vec<DbColumn>> {
        // WARN: SQLite only!
        let statement = format!(r#"SELECT * FROM pragma_table_info("{}");"#, self.table_name);
        Ok(self
            .query(statement.as_str(), connection)
            .await?
            .iter()
            .map(|row| DbColumn {
                name: row.get_string("name"),
            })
            .collect())
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
}

#[derive(Subcommand, Debug)]
pub enum GetSubcommand {
    /// Get the column header and rows from a given table.
    Table {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

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
pub fn print_text(rows: Vec<Vec<String>>) -> Result<()> {
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
    limit: &usize,
    offset: &usize,
) -> Result<()> {
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl.from(table_name).limit(limit).offset(offset);

    let columns = select.fetch_columns(&rltbl.connection).await?;
    let header = columns
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<String>>();
    let mut rows = select.fetch_all(&rltbl.connection).await?.vec_into();
    rows.insert(0, header);
    print_text(rows)?;
    Ok(())
}

// Print rows of a table, without column header.
pub async fn print_rows(_cli: &Cli, table_name: &str, limit: &usize, offset: &usize) -> Result<()> {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl.from(table_name).limit(limit).offset(offset);

    let rows = select.fetch_all(&rltbl.connection).await?.vec_into();
    print_text(rows)?;
    Ok(())
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) -> Result<()> {
    tracing::debug!("print_value({cli:?}, {table}, {row}, {column})");
    unimplemented!();
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
                limit,
                offset,
            } => print_table(&cli, table, limit, offset).await,
            GetSubcommand::Rows {
                table,
                limit,
                offset,
            } => print_rows(&cli, table, limit, offset).await,
            GetSubcommand::Value { table, row, column } => {
                print_value(&cli, table, *row, column).await
            }
        },
    }
}
