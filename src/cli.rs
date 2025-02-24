//! # rltbl/relatable
//!
//! This is relatable (rltbl::cli)

use crate as rltbl;
use rltbl::{
    core::{Change, ChangeAction, ChangeSet, Format, Relatable, MOVE_INTERVAL},
    sql::{JsonRow, VecInto},
    web::{serve, serve_cgi},
};

use ansi_term::Style;
use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use promptly::prompt_default;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use regex::Regex;
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
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
    /// Location of the database.
    #[arg(long,
          default_value = rltbl::core::RLTBL_DEFAULT_DB,
          action = ArgAction::Set,
          env = "RLTBL_DATABASE")]
    database: String,

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

    /// Undo changes to the database
    Undo {},

    /// Redo changes to the database that have been undone
    Redo {},

    /// Show recent changes to the database
    History {
        #[arg(long, value_name = "CONTEXT", action = ArgAction::Set,
              help = "Number of lines of redo / undo context (0 = infinite)",
              default_value_t = 5)]
        context: usize,
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

        /// Instruct the server to exit after this many seconds. Defaults to 0, i.e., no timeout.
        #[arg(long, default_value="0", action = ArgAction::Set)]
        timeout: usize,
    },

    /// Run Relatable as a CGI script
    Cgi {},

    /// Generate a demonstration database
    Demo {
        /// Overwrite an existing database
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,

        #[arg(long, value_name = "SIZE", action = ArgAction::Set,
              help = "Number of rows of demo data to generate",
              default_value_t = 1000)]
        size: usize,
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

pub async fn init(_cli: &Cli, force: &bool, path: &str) {
    match Relatable::init(force, Some(path)).await {
        Ok(_) => println!("Initialized a relatable database in '{path}'"),
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
    cli: &Cli,
    table_name: &str,
    filters: &Vec<String>,
    format: &str,
    limit: &usize,
    offset: &usize,
) {
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
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
pub async fn print_rows(cli: &Cli, table_name: &str, limit: &usize, offset: &usize) {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let select = rltbl.from(table_name).limit(limit).offset(offset);
    let rows = rltbl.fetch_json_rows(&select).await.unwrap().vec_into();
    print_text(&rows);
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) {
    tracing::debug!("print_value({cli:?}, {table}, {row}, {column})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
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

pub async fn print_history(cli: &Cli, context: usize) {
    tracing::debug!("print_history({cli:?}, {context})");

    let user = get_username(&cli);
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();

    fn get_content_as_string(change_json: &JsonRow) -> String {
        let content = change_json.get_string("content").expect("No content found");
        let content = Change::many_from_str(&content).expect("Could not parse content");
        content
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    // TODO: Need to come up with a more efficient way of doing this. The trouble is
    // that currently the get_user_history() function treats its context argument naively. To
    // properly retrieve "the last N actions" we need to distinguish between undos and dos
    // and only count the dos. Unfortunately this isn't straightforward because it needs to be
    // done in SQL. For the time being we pass None here to get the entire history, and then
    // stop after printing `context` records.
    let (mut undoable_changes, redoable_changes) = rltbl
        .get_user_history(&user, None)
        .await
        .expect("Could not get history");
    let next_undo = match undoable_changes.len() {
        0 => 0,
        _ => undoable_changes[0]
            .get_unsigned("change_id")
            .expect("No change_id found"),
    };
    undoable_changes.reverse();
    for (i, undo) in undoable_changes.iter().enumerate() {
        if i > context {
            break;
        }
        let change_id = undo.get_unsigned("change_id").expect("No change_id found");
        if change_id == next_undo {
            let undo_content = get_content_as_string(undo);
            let line = format!("▲ {undo_content}");
            println!("{}", Style::new().bold().paint(line));
        } else {
            let undo_content = get_content_as_string(undo);
            println!("  {undo_content}");
        }
    }
    let next_redo = match redoable_changes.len() {
        0 => 0,
        _ => redoable_changes[0]
            .get_unsigned("change_id")
            .expect("No change_id found"),
    };
    for (i, redo) in redoable_changes.iter().enumerate() {
        if i > context {
            break;
        }
        let change_id = redo.get_unsigned("change_id").expect("No change_id found");
        if change_id == next_redo {
            let redo_content = get_content_as_string(redo);
            println!("▼ {redo_content}");
        } else {
            let redo_content = get_content_as_string(redo);
            println!("  {redo_content}");
        }
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
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();

    // Fetch the current value from the db:
    let sql = format!(r#"SELECT "{column}" FROM "{table}" WHERE "_id" = ?"#);
    let params = json!([row]);
    let before = rltbl
        .connection
        .query_value(&sql, Some(&params))
        .await
        .expect("Error getting value")
        .expect("No value found");

    // Apply the change to the new value:
    let num_changes = rltbl
        .set_values(&ChangeSet {
            user: get_username(&cli),
            action: ChangeAction::Do,
            table: table.to_string(),
            description: "Set one value".to_string(),
            changes: vec![Change::Update {
                row,
                column: column.to_string(),
                before: before,
                after: to_value(value).unwrap_or_default(),
            }],
        })
        .await
        .unwrap()
        .changes
        .len();

    if num_changes < 1 {
        std::process::exit(1);
    }
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

pub async fn prompt_for_json_row(rltbl: &Relatable, table: &str) -> Result<JsonRow> {
    let columns = rltbl
        .fetch_columns(table)
        .await?
        .iter()
        .map(|c| c.name.to_string())
        .collect::<Vec<_>>();
    let columns = columns.iter().map(|c| c.as_str()).collect::<Vec<_>>();
    let mut json_row = JsonRow::from_strings(&columns);

    let prompt_for_column_value = |column: &str| -> JsonValue {
        let value: String =
            prompt_default(format!("Enter the value for '{column}':"), "".to_string())
                .expect("Error getting column value from user input");
        json!(value)
    };

    for column in columns {
        json_row
            .content
            .insert(column.to_string(), prompt_for_column_value(&column));
    }

    Ok(json_row)
}

pub async fn add_row(cli: &Cli, table: &str, after_id: Option<usize>) {
    tracing::debug!("add_row({cli:?}, {table}, {after_id:?})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let json_row = match &cli.input {
        Some(s) if s == "JSON" => input_json_row(),
        Some(s) => panic!("Unsupported input type '{s}'"),
        None => prompt_for_json_row(&rltbl, table)
            .await
            .expect("Error getting user input"),
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
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let user = get_username(&cli);
    let new_order = rltbl
        .move_row(table, &user, row, after_id)
        .await
        .expect("Failed to move row");
    if new_order > 0 {
        tracing::info!("Moved row {row} after row {after_id}");
    } else {
        std::process::exit(1);
    }
}

pub async fn delete_row(cli: &Cli, table: &str, row: usize) {
    tracing::debug!("delete_row({cli:?}, {table}, {row})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let user = get_username(&cli);
    let num_deleted = rltbl
        .delete_row(table, &user, row)
        .await
        .expect("Failed to delete row");
    if num_deleted > 0 {
        tracing::info!("Deleted row {row}");
    } else {
        std::process::exit(1);
    }
}

pub async fn undo(cli: &Cli) {
    tracing::debug!("undo({cli:?})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let user = get_username(&cli);
    let changeset = rltbl.undo(&user).await.expect("Failed to undo");
    if let None = changeset {
        std::process::exit(1);
    }
    tracing::info!("Last operation undone");
}

pub async fn redo(cli: &Cli) {
    tracing::debug!("redo({cli:?})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    let user = get_username(&cli);
    let changeset = rltbl.redo(&user).await.expect("Failed to redo");
    if let None = changeset {
        std::process::exit(1);
    }
    tracing::info!("Last operation redone");
}

pub async fn load_table(cli: &Cli, path: &str) {
    tracing::debug!("load_table({cli:?}, {path})");
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();

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
    let rltbl = Relatable::connect(Some(&cli.database)).await.unwrap();
    rltbl.save_all().await.expect("Error saving all");
}

pub async fn build_demo(cli: &Cli, force: &bool, size: usize) {
    tracing::debug!("build_demo({cli:?}");

    let rltbl = Relatable::init(force, Some(&cli.database))
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
    for i in 1..=size {
        let id = i;
        let order = i * MOVE_INTERVAL;
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
        Command::Init { force } => init(&cli, force, &cli.database).await,
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
        Command::Undo {} => undo(&cli).await,
        Command::Redo {} => redo(&cli).await,
        Command::History { context } => print_history(&cli, *context).await,
        Command::Load { subcommand } => match subcommand {
            LoadSubcommand::Table { path } => load_table(&cli, path).await,
        },
        Command::Save {} => save_all(&cli).await,
        Command::Serve {
            host,
            port,
            timeout,
        } => serve(&cli, host, port, timeout)
            .await
            .expect("Operation: 'serve' failed"),
        Command::Cgi {} => serve_cgi().await,
        Command::Demo { force, size } => build_demo(&cli, force, *size).await,
    }
}
