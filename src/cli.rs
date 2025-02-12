//! # rltbl/relatable
//!
//! This is relatable (rltbl::cli)

use crate::{
    core::{Change, ChangeAction, ChangeSet, Format, Relatable},
    sql::{JsonRow, VecInto},
    web::{serve, serve_cgi},
};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use promptly::prompt_default;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use regex::Regex;
use serde_json::{json, to_string_pretty, to_value, Map as JsonMap, Value as JsonValue};
use std::{io, io::Write, path::Path};
use tabwriter::TabWriter;

static COLUMN_HELP: &str = "A column name or label";
static ROW_HELP: &str = "A row number";
static TABLE_HELP: &str = "A table name";
static VALUE_HELP: &str = "A value for a cell";

#[derive(Parser, Debug)]
#[command(version,
          about = "Relatable (rltbl): Connect your data!",
          long_about = None)]
pub struct Cli {
    #[arg(long, default_value="", action = ArgAction::Set, env = "RLTBL_USER")]
    user: String,

    /// Can be one of: JSON (that's it for now). If unspecified Valve will attempt to read the
    /// environment variable RLTBL_INPUT. If that is also unset, the user will be presented with
    /// questions whenever input is required.
    #[arg(long, action = ArgAction::Set, env = "RLTBL_INPUT")]
    pub input: Option<String>,

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
    /// Initialize a database
    Init {
        /// Overwrite an existing database
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },

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

    /// Add data to the database
    Add {
        #[command(subcommand)]
        subcommand: AddSubcommand,
    },

    /// Move data around within a data table
    Move {
        #[command(subcommand)]
        subcommand: MoveSubcommand,
    },

    /// Delete data from the database
    Delete {
        #[command(subcommand)]
        subcommand: DeleteSubcommand,
    },

    /// Load data into the datanase
    Load {
        #[command(subcommand)]
        subcommand: LoadSubcommand,
    },

    /// Save the data
    Save {},

    /// Run a Relatable server
    Serve {
        /// Server host address
        #[arg(long, default_value="0.0.0.0", action = ArgAction::Set)]
        host: String,

        /// Server port
        #[arg(long, default_value="0", action = ArgAction::Set)]
        port: u16,
    },

    /// Run Relatable as a CGI script
    Cgi {},

    /// Generate a demonstration database
    Demo {
        /// Overwrite an existing database
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

#[derive(Subcommand, Debug)]
pub enum AddSubcommand {
    Row {
        #[arg(long, action = ArgAction::Set)]
        after_id: Option<usize>,

        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum MoveSubcommand {
    Row {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "AFTER", action = ArgAction::Set,
              help = "The ID of the row after which this one is to be moved")]
        after: usize,
    },
}

#[derive(Subcommand, Debug)]
pub enum DeleteSubcommand {
    Row {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,
    },
}

#[derive(Subcommand, Debug)]
pub enum LoadSubcommand {
    Table {
        #[arg(value_name = "PATH", action = ArgAction::Set, help = "The path to load from")]
        path: String,
    },
}

pub async fn init(_cli: &Cli, force: &bool) {
    match Relatable::init(force).await {
        Ok(_) => (),
        Err(err) => panic!("{err:?}"),
    }
}

/// Given a vector of vectors of strings,
/// print text with "elastic tabstops".
pub fn print_text(rows: &Vec<Vec<String>>) {
    let mut tw = TabWriter::new(vec![]);
    for row in rows {
        tw.write(format!("{}\n", row.join("\t")).as_bytes())
            .unwrap();
    }
    tw.flush().unwrap();
    let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    print!("{written}");
}

pub fn print_tsv(rows: Vec<Vec<String>>) {
    for row in rows {
        println!("{}", row.join("\t"));
    }
}

// Print a table with its column header.
pub async fn print_table(
    _cli: &Cli,
    table_name: &str,
    filters: &Vec<String>,
    format: &str,
    limit: &usize,
    offset: &usize,
) {
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::connect(None).await.unwrap();
    let select = rltbl
        .from(table_name)
        .filters(filters)
        .unwrap()
        .limit(limit)
        .offset(offset);
    match format.to_lowercase().as_str() {
        "json" => {
            let json = json!(rltbl.fetch(&select).await.unwrap());
            print!("{}", to_string_pretty(&json).unwrap());
        }
        "text" | "" => {
            print!("{}", rltbl.fetch(&select).await.unwrap().to_string());
        }
        _ => unimplemented!("output format {format}"),
    };

    tracing::debug!("Processed: {}", {
        let format = Format::try_from(&format.to_string()).unwrap();
        let url = select.to_url("/table", &format).unwrap();
        url
    });
}

// Print rows of a table, without column header.
pub async fn print_rows(_cli: &Cli, table_name: &str, limit: &usize, offset: &usize) {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::connect(None).await.unwrap();
    let select = rltbl.from(table_name).limit(limit).offset(offset);
    let rows = rltbl.fetch_json_rows(&select).await.unwrap().vec_into();
    print_text(&rows);
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) {
    tracing::debug!("print_value({cli:?}, {table}, {row}, {column})");
    let rltbl = Relatable::connect(None).await.unwrap();
    let statement = format!(r#"SELECT "{column}" FROM "{table}" WHERE _id = ?"#);
    let params = json!([row]);
    if let Some(value) = rltbl
        .connection
        .query_value(&statement, Some(&params))
        .await
        .unwrap()
    {
        let text = match value {
            JsonValue::String(value) => value.to_string(),
            value => format!("{value}"),
        };
        println!("{text}");
    }
}

// Get the user from the CLI, RLTBL_USER environment variable,
// or the general environment.
pub fn get_username(cli: &Cli) -> String {
    let mut username = cli.user.clone();
    if username == "" {
        username = whoami::username();
    }
    username
}

pub async fn set_value(cli: &Cli, table: &str, row: usize, column: &str, value: &str) {
    tracing::debug!("set_value({cli:?}, {table}, {row}, {column}, {value})");
    let rltbl = Relatable::connect(None).await.unwrap();
    rltbl
        .set_values(&ChangeSet {
            user: get_username(&cli),
            action: ChangeAction::Do,
            table: table.to_string(),
            description: "Set one value".to_string(),
            changes: vec![Change::Update {
                row,
                column: column.to_string(),
                value: to_value(value).unwrap_or_default(),
            }],
        })
        .await
        .unwrap();
}

pub fn input_json_row() -> JsonRow {
    let mut json_row = String::new();
    io::stdin()
        .read_line(&mut json_row)
        .expect("Error reading from STDIN");
    let json_row = serde_json::from_str::<JsonValue>(&json_row)
        .expect(&format!("Invalid JSON: {json_row}"))
        .as_object()
        .expect(&format!("{json_row} is not a JSON object"))
        .clone();
    JsonRow { content: json_row }
}

pub fn prompt_for_json_row() -> JsonRow {
    // The content of the row to be returned:
    let mut json_map = JsonMap::new();

    let prompt_for_column_name = || -> String {
        let column: String = prompt_default(
            "Enter the name of the next column, or press enter to stop adding values for columns:",
            "".to_string(),
        )
        .expect("Error getting column from user input");
        column
    };
    let prompt_for_column_value = |column: &str| -> JsonValue {
        let value: String =
            prompt_default(format!("Enter the value for '{column}':"), "".to_string())
                .expect("Error getting column value from user input");
        json!(value)
    };

    let mut column = prompt_for_column_name();
    while column != "" {
        json_map.insert(column.to_string(), prompt_for_column_value(&column));
        column = prompt_for_column_name();
    }

    JsonRow { content: json_map }
}

pub async fn add_row(cli: &Cli, table: &str, after_id: Option<usize>) {
    tracing::debug!("add_row({cli:?}, {table}, {after_id:?})");
    let rltbl = Relatable::connect(None).await.unwrap();
    let json_row = match &cli.input {
        Some(s) if s == "JSON" => input_json_row(),
        Some(s) => panic!("Unsupported input type '{s}'"),
        None => prompt_for_json_row(),
    };

    if json_row.content.is_empty() {
        panic!("Cannot insert an empty row to the database");
    }

    let user = get_username(&cli);
    let row = rltbl
        .add_row(table, &user, after_id, &json_row)
        .await
        .expect("Error adding row");
    tracing::info!("Added row {}", row.order);
}

pub async fn move_row(cli: &Cli, table: &str, row: usize, after_id: usize) {
    tracing::debug!("move_row({cli:?}, {table}, {row}, {after_id})");
    let rltbl = Relatable::connect(None).await.unwrap();
    let user = get_username(&cli);
    rltbl
        .move_row(table, &user, row, after_id)
        .await
        .expect("Failed to move row");
    tracing::info!("Moved row {row} after row {after_id}");
}

pub async fn delete_row(cli: &Cli, table: &str, row: usize) {
    tracing::debug!("delete_row({cli:?}, {table}, {row})");
    let rltbl = Relatable::connect(None).await.unwrap();
    let user = get_username(&cli);
    rltbl
        .delete_row(table, &user, row)
        .await
        .expect("Failed to delete row");
    tracing::info!("Deleted row {row}");
}

pub async fn load_table(cli: &Cli, path: &str) {
    tracing::debug!("load_table({cli:?}, {path})");
    let rltbl = Relatable::connect(None).await.unwrap();

    // We will use this pattern to normalize the table name:
    let pattern = Regex::new(r#"[^0-9a-zA-Z_]+"#).expect("Invalid regex pattern");
    let table = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("Error writing to path");
    let table = pattern.replace_all(table, "_").to_string();
    // Now replace any trailing or leading underscores:
    let table = table.trim_end_matches("_");
    let table = table.trim_start_matches("_");

    rltbl
        .load_table(&table, path)
        .await
        .expect("Error loading table");
    tracing::info!("Loaded table '{table}'");
}

pub async fn save_all(cli: &Cli) {
    tracing::debug!("save_all({cli:?})");
    let rltbl = Relatable::connect(None).await.unwrap();
    rltbl.save_all().await.expect("Error saving all");
}

pub async fn build_demo(cli: &Cli, force: &bool) {
    tracing::debug!("build_demo({cli:?}");

    let rltbl = Relatable::init(force)
        .await
        .expect("Database was initialized");

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('penguin', 'penguin.tsv')"#;
    rltbl.connection.query(sql, None).await.unwrap();

    // Create the penguin table.
    let sql = r#"CREATE TABLE penguin (
      _id INTEGER UNIQUE,
      _order INTEGER UNIQUE,
      study_name TEXT,
      sample_number TEXT,
      species TEXT,
      island TEXT,
      individual_id TEXT,
      culmen_length TEXT,
      body_mass TEXT
    )"#;
    rltbl.connection.query(sql, None).await.unwrap();

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
        let sql = r#"INSERT INTO "penguin"
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
        rltbl.connection.query(&sql, Some(&params)).await.unwrap();
    }
}

pub async fn process_command() {
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
        Command::Init { force } => init(&cli, force).await,
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
        Command::Add { subcommand } => match subcommand {
            AddSubcommand::Row { table, after_id } => add_row(&cli, table, *after_id).await,
        },
        Command::Move { subcommand } => match subcommand {
            MoveSubcommand::Row { table, row, after } => move_row(&cli, table, *row, *after).await,
        },
        Command::Delete { subcommand } => match subcommand {
            DeleteSubcommand::Row { table, row } => delete_row(&cli, table, *row).await,
        },
        Command::Load { subcommand } => match subcommand {
            LoadSubcommand::Table { path } => load_table(&cli, path).await,
        },
        Command::Save {} => save_all(&cli).await,
        Command::Serve { host, port } => serve(&cli, host, port)
            .await
            .expect("Operation: 'serve' failed"),
        Command::Cgi {} => serve_cgi().await,
        Command::Demo { force } => build_demo(&cli, force).await,
    }
}
