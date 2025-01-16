use crate::{
    core::{Change, ChangeAction, ChangeSet, Relatable, RelatableError},
    sql::{query, query_value, VecInto},
    web::{serve, serve_cgi},
};

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use serde_json::{json, to_string_pretty, to_value, Value as JsonValue};
use std::{io::Write, path::Path as FilePath};
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

// TODO: In general I think it is better, for CLI, to unwrap/panic/expect instead of returning
// an Error, since only the former provides you with context and allows for a stack trace.

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

pub async fn process_command() {
    // Handle a CGI request, instead of normal CLI input.
    match std::env::var_os("GATEWAY_INTERFACE").and_then(|p| Some(p.into_string())) {
        Some(Ok(s)) if s == "CGI/1.1" => {
            return serve_cgi().await.expect("Failed to serve CGI");
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
            } => print_table(&cli, table, filters, format, limit, offset)
                .await
                .expect("Operation: 'print table' failed"),
            GetSubcommand::Rows {
                table,
                limit,
                offset,
            } => print_rows(&cli, table, limit, offset)
                .await
                .expect("Operation: 'print rows' failed"),
            GetSubcommand::Value { table, row, column } => print_value(&cli, table, *row, column)
                .await
                .expect("Operation: 'print value' failed"),
        },
        Command::Set { subcommand } => match subcommand {
            SetSubcommand::Value {
                table,
                row,
                column,
                value,
            } => set_value(&cli, table, *row, column, value)
                .await
                .expect("Operation: 'set value' failed"),
        },
        Command::Serve { host, port } => serve(&cli, host, port)
            .await
            .expect("Operation: 'serve' failed"),
        Command::Demo { force } => build_demo(&cli, force)
            .await
            .expect("Operation: 'build demo' failed"),
    }
}
