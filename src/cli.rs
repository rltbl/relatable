//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[cli](crate::cli))

use crate as rltbl;
use rltbl::{
    core::{Change, ChangeAction, ChangeSet, Relatable},
    select::{Format, Select},
    sql,
    sql::{CachingStrategy, JsonRow, SqlParam, VecInto},
    web::{serve, serve_cgi},
};

use ansi_term::Style;
use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use promptly::prompt_opt;
use regex::Regex;
use serde_json::{json, to_string_pretty, Value as JsonValue};
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
    #[arg(long, action = ArgAction::Set, env = "RLTBL_CONNECTION")]
    database: Option<String>,

    #[arg(long, action = ArgAction::Set, env = "RLTBL_USER")]
    user: Option<String>,

    /// Can be one of: JSON (that's it for now). If unspecified Valve will attempt to read the
    /// environment variable RLTBL_INPUT. If that is also unset, the user will be presented with
    /// questions whenever input is required.
    #[arg(long, action = ArgAction::Set, env = "RLTBL_INPUT")]
    pub input: Option<String>,

    #[command(flatten)]
    verbose: Verbosity,

    /// One of: none, truncate, truncate_all, trigger, memory
    #[arg(long, default_value = "none", action = ArgAction::Set)]
    pub caching: CachingStrategy,

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
    Save {
        /// The directory to which to save the table .TSVs (defaults to the value of "path" from
        /// the table table entry for each table when not set)
        #[arg(value_name = "SAVE_DIR", action = ArgAction::Set)]
        save_dir: Option<String>,
    },

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

        /// Output format: text, vertical, JSON, TSV
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

    /// Read a JSON-formatted string representing a row (of the form: { "level": LEVEL,
    /// "rule": RULE, "message": MESSAGE}) from STDIN and add it to the message table.
    Message {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: String,
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

    Message {
        #[arg(long,
              value_name = "RULE",
              action = ArgAction::Set,
              help = "Only delete messages from the given row or column whose rule matches RULE, \
                      which may contain SQL-style wildcards.")]
        rule: Option<String>,

        #[arg(long,
              value_name = "USER",
              action = ArgAction::Set,
              help = "Only delete messages from the given row or column that were added by USER.")]
        user: Option<String>,

        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: Option<usize>,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum LoadSubcommand {
    Table {
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,

        #[arg(long, action = ArgAction::SetTrue)]
        validate: bool,

        #[arg(value_name = "PATH", num_args=1..,
              action = ArgAction::Set,
              help = "The path(s) to load from")]
        paths: Vec<String>,
    },
}

pub async fn init(cli: &Cli, force: &bool, path: Option<&str>) {
    tracing::trace!("init({cli:?}, {force}, {path:?})");
    match Relatable::init(force, path, &cli.caching).await {
        Ok(_) => println!(
            "Initialized a relatable database in '{}'",
            match path {
                None => rltbl::core::RLTBL_DEFAULT_DB,
                Some(db) => db,
            }
        ),
        Err(err) => panic!("{err:?}"),
    }
}

/// Given a vector of vectors of strings,
/// print text with "elastic tabstops".
pub fn print_text(rows: &Vec<Vec<String>>) {
    tracing::trace!("print_text({rows:?})");
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
    tracing::trace!("print_tsv({rows:?})");
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
    tracing::trace!("print_table({cli:?}, {table_name}, {filters:?}, {format}, {limit}, {offset})");
    // TODO: We need to ouput round numbers consistently between PostgreSQL and SQLite.
    // Currently, for instance, 37 is displayed as 37.0 in SQLite and 37 in PostgreSQL.
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let select = Select::from(table_name)
        .filters(filters)
        .unwrap()
        .limit(limit)
        .offset(offset);
    match format.to_lowercase().as_str() {
        "json" => {
            let json = json!(rltbl.fetch(&select).await.unwrap());
            print!("{}", to_string_pretty(&json).unwrap());
        }
        "vertical" => {
            println!("{table_name}\n-----");
            for row in rltbl.fetch(&select).await.unwrap().rows {
                for (column, value) in row.cells.iter() {
                    println!("{column}: {value}", value = value.text);
                }
                println!("-----");
            }
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
    tracing::trace!("print_rows({cli:?}, {table_name}, {limit}, {offset})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let select = Select::from(table_name).limit(limit).offset(offset);
    let rows = rltbl.fetch_rows(&select).await.unwrap().vec_into();
    print_text(&rows);
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) {
    tracing::trace!("print_value({cli:?}, {table}, {row}, {column})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let statement = format!(
        r#"SELECT "{column}" FROM "{table}" WHERE _id = {sql_param}"#,
        sql_param = sql::SqlParam::new(&rltbl.connection.kind()).next(),
    );
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
    tracing::trace!("print_history({cli:?}, {context})");
    fn get_content_as_string(change_json: &JsonRow) -> String {
        let content = change_json.get_string("content").expect("No content found");
        let content = Change::many_from_str(&content).expect("Could not parse content");
        content
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    let user = get_username(&cli);
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let history = rltbl
        .get_user_history(
            &user,
            match context {
                0 => None,
                _ => Some(context),
            },
        )
        .await
        .expect("Could not get history");

    let (undoable_changes, mut redoable_changes) = (
        history.changes_done_stack.clone(),
        history.changes_undone_stack.clone(),
    );

    let next_redo = match redoable_changes.len() {
        0 => 0,
        _ => redoable_changes[0]
            .get_unsigned("change_id")
            .expect("No change_id found"),
    };
    redoable_changes.reverse();
    for (i, change) in redoable_changes.iter().enumerate() {
        if i > context {
            break;
        }
        let change_id = change
            .get_unsigned("change_id")
            .expect("No change_id found");
        let action = change.get_string("action").expect("No action found");
        if change_id == next_redo {
            let change_content = get_content_as_string(change);
            println!("▲ {change_content} (action #{change_id}, {action})");
        } else {
            let change_content = get_content_as_string(change);
            println!("  {change_content} (action #{change_id}, {action})");
        }
    }
    let next_undo = match undoable_changes.len() {
        0 => 0,
        _ => undoable_changes[0]
            .get_unsigned("change_id")
            .expect("No change_id found"),
    };
    for (i, change) in undoable_changes.iter().enumerate() {
        if i > context {
            break;
        }
        let change_id = change
            .get_unsigned("change_id")
            .expect("No change_id found");
        let action = change.get_string("action").expect("No action found");
        if change_id == next_undo {
            let change_content = get_content_as_string(change);
            let line = format!("▼ {change_content} (action #{change_id}, {action})");
            println!("{}", Style::new().bold().paint(line));
        } else {
            let change_content = get_content_as_string(change);
            println!("  {change_content} (action #{change_id}, {action})");
        }
    }
}

// Get the user from the CLI, RLTBL_USER environment variable,
// or the general environment.
pub fn get_username(cli: &Cli) -> String {
    tracing::trace!("get_username({cli:?})");
    match &cli.user {
        Some(user) => user.clone(),
        None => whoami::username(),
    }
}

pub async fn set_value(cli: &Cli, table: &str, row: usize, column: &str, value: &str) {
    tracing::trace!("set_value({cli:?}, {table}, {row}, {column}, {value})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();

    // Fetch the current value from the db:
    let sql = format!(
        r#"SELECT "{column}" FROM "{table}" WHERE "_id" = {sql_param}"#,
        sql_param = SqlParam::new(&rltbl.connection.kind()).next()
    );
    let params = json!([row]);
    let before = rltbl
        .connection
        .query_value(&sql, Some(&params))
        .await
        .expect("Error getting value")
        .expect("No value found");
    let after = serde_json::from_str::<JsonValue>(value).unwrap_or(json!(value));

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
                after: after,
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
    tracing::trace!("input_json_row()");
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

pub fn prompt_for_column_value(column: &str) -> JsonValue {
    tracing::trace!("prompt_for_column_value({column})");
    let value: Option<String> = prompt_opt(format!("Enter a {column}"))
        .expect("Error getting column value from user input");
    match value {
        Some(value) => serde_json::from_str::<JsonValue>(&value).unwrap_or(json!(&value)),
        None => json!(""),
    }
}

pub async fn prompt_for_json_message(
    rltbl: &Relatable,
    table: &str,
    row: usize,
    column: &str,
) -> Result<JsonRow> {
    tracing::trace!("prompt_for_json_message({rltbl:?}, {table}, {row}, {column})");
    let columns = rltbl
        .fetch_columns("message")
        .await?
        .iter()
        .filter(|c| !["message_id", "added_by"].contains(&c.name.as_str()))
        .map(|c| c.name.to_string())
        .collect::<Vec<_>>();
    let columns = columns.iter().map(|c| c.as_str()).collect::<Vec<_>>();
    let mut json_row = JsonRow::from_strings(&columns);

    json_row.content.insert("table".to_string(), json!(table));
    json_row.content.insert("row".to_string(), json!(row));
    json_row.content.insert("column".to_string(), json!(column));
    tracing::debug!("Received json row from user input: {json_row:?}");

    for column in columns {
        if let Some(JsonValue::Null) = json_row.content.get(column) {
            json_row
                .content
                .insert(column.to_string(), prompt_for_column_value(&column));
        }
    }

    Ok(json_row)
}

pub async fn prompt_for_json_row(rltbl: &Relatable, table_name: &str) -> Result<JsonRow> {
    tracing::trace!("prompt_for_json_row({rltbl:?}, {table_name})");
    let columns = rltbl
        .fetch_columns(table_name)
        .await?
        .iter()
        .map(|c| c.name.to_string())
        .collect::<Vec<_>>();
    let columns = columns.iter().map(|c| c.as_str()).collect::<Vec<_>>();
    let mut json_row = JsonRow::from_strings(&columns);

    for column in &columns {
        json_row
            .content
            .insert(column.to_string(), prompt_for_column_value(&column));
    }

    Ok(json_row)
}

/// Use Relatable, in conformity with the given command-line parameters, to add a row representing
/// a [Message](rltbl::table::Message) to the message table. The details of the message are read
/// from STDIN, either interactively or in JSON format.
pub async fn add_message(cli: &Cli, table: &str, row: usize, column: &str) {
    tracing::trace!("add_message({cli:?}, {table:?}, {row:?}, {column:?})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let json_message = match &cli.input {
        Some(s) if s == "JSON" => input_json_row(),
        Some(s) => panic!("Unsupported input type '{s}'"),
        None => prompt_for_json_message(&rltbl, table, row, column)
            .await
            .expect("Error getting user input"),
    };

    if json_message.content.is_empty() {
        panic!("Refusing to insert an empty message to the database");
    }

    let value = json!(json_message
        .content
        .get("value")
        .and_then(|m| m.as_str())
        .expect("The field 'value' (type: string) is required."));
    let level = json_message
        .content
        .get("level")
        .and_then(|l| l.as_str())
        .expect("The field 'level' (type: string) is required.");
    let rule = json_message
        .content
        .get("rule")
        .and_then(|r| r.as_str())
        .expect("The field 'rule' (type: string) is required.");
    let message = json_message
        .content
        .get("message")
        .and_then(|m| m.as_str())
        .expect("The field 'message' (type: string) is required.");

    let user = get_username(&cli);
    let (mid, message) = rltbl
        .add_message(&user, table, row, column, &value, &level, &rule, &message)
        .await
        .expect("Error adding row");
    tracing::info!("Added message (ID: {mid}) {message:?}");
}

pub async fn add_row(cli: &Cli, table: &str, after_id: Option<usize>) {
    tracing::trace!("add_row({cli:?}, {table}, {after_id:?})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
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
    tracing::trace!("move_row({cli:?}, {table}, {row}, {after_id})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
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
    tracing::trace!("delete_row({cli:?}, {table}, {row})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
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

pub async fn delete_message(
    cli: &Cli,
    target_rule: Option<&str>,
    target_user: Option<&str>,
    table: &str,
    row: Option<usize>,
    column: Option<&str>,
) {
    tracing::trace!(
        "delete_message({cli:?}, {target_rule:?}, {target_user:?}, {table}, {row:?}, {column:?})"
    );
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let num_deleted = rltbl
        .delete_message(table, row, column, target_rule, target_user)
        .await
        .expect("Failed to delete message");
    if num_deleted > 0 {
        tracing::info!("Deleted {num_deleted} message(s)");
    } else {
        std::process::exit(1);
    }
}

pub async fn undo(cli: &Cli) {
    tracing::trace!("undo({cli:?})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let user = get_username(&cli);
    let changeset = rltbl.undo(&user).await.expect("Failed to undo");
    if let None = changeset {
        std::process::exit(1);
    }
    tracing::info!("Last operation undone");
}

pub async fn redo(cli: &Cli) {
    tracing::trace!("redo({cli:?})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    let user = get_username(&cli);
    let changeset = rltbl.redo(&user).await.expect("Failed to redo");
    if let None = changeset {
        std::process::exit(1);
    }
    tracing::info!("Last operation redone");
}

pub async fn load_tables(cli: &Cli, paths: &Vec<String>, force: bool, validate: bool) {
    tracing::trace!("load_tables({cli:?}, {paths:?})");
    for path in paths {
        load_table(cli, &path, force, validate).await;
    }
}

pub async fn load_table(cli: &Cli, path: &str, force: bool, validate: bool) {
    tracing::trace!("load_table({cli:?}, {path})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();

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

    rltbl.load_table(&table, path, force, validate).await;
    tracing::info!("Loaded table '{table}'");
}

pub async fn save_all(cli: &Cli, save_dir: Option<&str>) {
    tracing::trace!("save_all({cli:?})");
    let rltbl = Relatable::connect(cli.database.as_deref(), &cli.caching)
        .await
        .unwrap();
    rltbl.save_all(save_dir).await.expect("Error saving all");
}

pub async fn build_demo(cli: &Cli, force: &bool, size: usize) {
    tracing::trace!("build_demo({cli:?}, {force}, {size})");
    Relatable::build_demo(cli.database.as_deref(), force, size, &cli.caching)
        .await
        .expect("Error building demonstration database");
    println!(
        "Created a demonstration database in '{}'",
        match &cli.database {
            None => rltbl::core::RLTBL_DEFAULT_DB,
            Some(db) => db,
        }
    );
}

pub async fn process_command() {
    tracing::trace!("process_command()");
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
        Command::Init { force } => init(&cli, force, cli.database.as_deref()).await,
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
            AddSubcommand::Message { table, row, column } => {
                add_message(&cli, table, *row, column).await
            }
        },
        Command::Move { subcommand } => match subcommand {
            MoveSubcommand::Row { table, row, after } => move_row(&cli, table, *row, *after).await,
        },
        Command::Delete { subcommand } => match subcommand {
            DeleteSubcommand::Row { table, row } => delete_row(&cli, table, *row).await,
            DeleteSubcommand::Message {
                rule,
                user,
                table,
                row,
                column,
            } => {
                delete_message(
                    &cli,
                    rule.as_deref(),
                    user.as_deref(),
                    table,
                    *row,
                    column.as_deref(),
                )
                .await
            }
        },
        Command::Undo {} => undo(&cli).await,
        Command::Redo {} => redo(&cli).await,
        Command::History { context } => print_history(&cli, *context).await,
        Command::Load { subcommand } => match subcommand {
            LoadSubcommand::Table {
                paths,
                force,
                validate,
            } => load_tables(&cli, paths, *force, *validate).await,
        },
        Command::Save { save_dir } => save_all(&cli, save_dir.as_deref()).await,
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
