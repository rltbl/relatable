use std::collections::HashMap;
use std::io::Write;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use sqlx::{
    any::{install_default_drivers, AnyRow},
    query, AnyPool, Column, Row,
};
use sqlx_core::any::AnyTypeInfoKind;
use tabwriter::TabWriter;

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
    DatabaseError(sqlx::Error),
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

pub struct Relatable {
    pub pool: AnyPool,
    pub default_limit: usize,
}

impl Relatable {
    pub async fn default() -> Result<Self> {
        install_default_drivers();
        let pool = AnyPool::connect("sqlite://.relatable/relatable.db").await?;
        Ok(Self {
            pool,
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

pub struct DbRow(AnyRow);

impl DbRow {
    /// Given a database row, the name of a column, and it's SQL type, return the value of that column
    /// from the given row as a String.
    pub fn get_string(&self, column_name: &str) -> String {
        let column = self.0.try_column(column_name);
        let kind = match column {
            Ok(column) => column.type_info().kind(),
            Err(_) => AnyTypeInfoKind::Null,
        };
        match kind {
            AnyTypeInfoKind::SmallInt | AnyTypeInfoKind::Integer | AnyTypeInfoKind::BigInt => {
                let value: i32 = self.0.try_get(column_name).unwrap_or_default();
                value.to_string()
            }
            AnyTypeInfoKind::Real | AnyTypeInfoKind::Double => {
                let value: f64 = self.0.try_get(column_name).unwrap_or_default();
                value.to_string()
            }
            AnyTypeInfoKind::Text => self.0.try_get(column_name).unwrap_or_default(),
            // AnyTypeInfoKind::Null,
            // AnyTypeInfoKind::Bool,
            // AnyTypeInfoKind::Blob,
            _ => "".to_string(),
        }
    }

    fn to_strings(&self) -> Vec<String> {
        let columns = self.0.columns();
        let mut result = vec![];
        for column in columns {
            result.push(self.get_string(column.name()));
        }
        result
    }
    fn to_map(&self) -> HashMap<String, String> {
        let columns = self.0.columns();
        let mut result = HashMap::new();
        for column in columns {
            result.insert(column.name().into(), self.get_string(column.name()));
        }
        result
    }
}

impl std::fmt::Display for DbRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_strings().join("\t"))
    }
}

impl std::fmt::Debug for DbRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_map())
    }
}

impl From<DbRow> for Vec<String> {
    fn from(row: DbRow) -> Self {
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

    pub async fn fetch_all(&self, pool: &AnyPool) -> Result<Vec<DbRow>> {
        query(self.to_sql()?.as_str())
            .map(|row| DbRow(row))
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn fetch_columns(&self, pool: &AnyPool) -> Result<Vec<DbColumn>> {
        // WARN: SQLite only!
        let sql = format!(r#"PRAGMA table_info("{}");"#, self.table_name);
        query(sql.as_str())
            .map(|row: AnyRow| DbColumn {
                name: row.try_get("name").unwrap_or_default(),
                // sqltype: row.try_get("type").unwrap_or_default(),
            })
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
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

    let columns = select.fetch_columns(&rltbl.pool).await?;
    let header = columns
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<String>>();
    let mut rows = select.fetch_all(&rltbl.pool).await?.vec_into();
    rows.insert(0, header);
    print_text(rows)?;
    Ok(())
}

// Print rows of a table, without column header.
pub async fn print_rows(_cli: &Cli, table_name: &str, limit: &usize, offset: &usize) -> Result<()> {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl.from(table_name).limit(limit).offset(offset);

    let rows = select.fetch_all(&rltbl.pool).await?.vec_into();
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
