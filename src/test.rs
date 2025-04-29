//! API tests

use rltbl::{
    core::{render_values, Filter, QueryParams, Relatable, Select, RLTBL_DEFAULT_DB},
    sql::{DbKind, SqlParam},
};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use rand::{
    distributions::{Distribution as _, Uniform},
    rngs::StdRng,
    SeedableRng as _,
};
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;

#[derive(Parser, Debug)]
#[command(version, about = "Relatable (rltbl): Connect your data!", long_about = None)]
pub struct Cli {
    /// Location of the database.
    #[arg(long,
          default_value = RLTBL_DEFAULT_DB,
          action = ArgAction::Set,
          env = "RLTBL_CONNECTION")]
    database: String,

    #[arg(long, default_value="", action = ArgAction::Set, env = "RLTBL_USER")]
    user: String,

    #[command(flatten)]
    verbose: Verbosity,

    #[arg(long, action = ArgAction::SetTrue)]
    vertical: bool,

    #[arg(long, action = ArgAction::Set)]
    seed: Option<u64>,

    // Subcommand:
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Generate a random sequence of relatable operations that can then be instantiated and
    /// executed by some external script as part of an end-to-end test.
    GenerateSeq {
        #[arg(action = ArgAction::Set)]
        table: String,

        #[arg(long, default_value = "10", action = ArgAction::Set)]
        min_length: usize,

        #[arg(long, default_value = "15", action = ArgAction::Set)]
        max_length: usize,
    },
    /// Test a joined query
    SelectTest {
        #[arg(action = ArgAction::Set)]
        table1: String,

        #[arg(action = ArgAction::Set)]
        column: String,

        #[arg(action = ArgAction::Set)]
        table2: String,

        #[arg(action = ArgAction::Set)]
        value: JsonValue,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DbOperation {
    Add,
    Delete,
    Update,
    Move,
    Undo,
    Redo,
}

impl std::fmt::Display for DbOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbOperation::Add => write!(f, "add"),
            DbOperation::Delete => write!(f, "delete"),
            DbOperation::Update => write!(f, "update"),
            DbOperation::Move => write!(f, "move"),
            DbOperation::Undo => write!(f, "undo"),
            DbOperation::Redo => write!(f, "redo"),
        }
    }
}

async fn generate_operation_sequence(
    cli: &Cli,
    rltbl: &Relatable,
    table: &str,
    min_length: usize,
    max_length: usize,
) {
    /*
    Algorithm:
    ----------
    1. Determine the number of "modify" operations to randomly generate. Then for each operation do
       the following:
    2. Generate a modify/undo pair
    3. Do the modify
    4. Either add an undo immediately after the given modify, or defer the undo by adding it to an
       undo stack.
    5. Possibly generate a redo/undo pair such that the undo comes immediately after the undo, or
       is deferred to the undo stack.
    6. Once all of the modify operations have been processed, go through the undo stack:
       a. For each undo, once it's been processed, possibly generate a redo/undo pair such that the
          undo comes immediately after the undo, or is deferred to the undo stack.

    After this function returns, the database should be in the same logical state as it was before.
     */

    fn random_between(min: usize, max: usize, seed: &mut i64) -> usize {
        let between = Uniform::from(min..max);
        let mut rng = if *seed < 0 {
            StdRng::from_entropy()
        } else {
            *seed += 10;
            StdRng::seed_from_u64(*seed as u64)
        };
        between.sample(&mut rng)
    }

    let mut seed: i64 = match cli.seed {
        None => -1,
        Some(seed) => seed as i64,
    };

    let list_len = random_between(min_length, max_length + 1, &mut seed);

    let mut num_rows_in_table = rltbl
        .connection
        .query_one(
            &format!(r#"SELECT COUNT(1) AS "count" FROM "{table}""#),
            None,
        )
        .await
        .expect("Error querying database")
        .unwrap()
        .get_unsigned("count")
        .expect("No count found");

    let mut operations = vec![];
    let mut undo_stack = vec![];
    for _ in 0..list_len {
        match random_between(0, 4, &mut seed) {
            0 => {
                num_rows_in_table += 1;
                operations.push(DbOperation::Add)
            }
            1 => {
                if num_rows_in_table == 0 {
                    num_rows_in_table += 1;
                    operations.push(DbOperation::Add)
                } else {
                    num_rows_in_table -= 1;
                    operations.push(DbOperation::Delete)
                }
            }
            2 => {
                if num_rows_in_table == 0 {
                    num_rows_in_table += 1;
                    operations.push(DbOperation::Add)
                } else {
                    operations.push(DbOperation::Update)
                }
            }
            3 => {
                if num_rows_in_table == 0 {
                    num_rows_in_table += 1;
                    operations.push(DbOperation::Add)
                } else {
                    operations.push(DbOperation::Move)
                }
            }
            _ => unreachable!(),
        };
        // Randomly either add an undo immediately after the modify, or add it to the undo_stack:
        if random_between(0, 2, &mut seed) == 0 {
            operations.push(DbOperation::Undo);
            // Randomly add a redo as well:
            if random_between(0, 2, &mut seed) == 0 {
                operations.push(DbOperation::Redo);
                // Randomly either add an undo either immediately after the redo, or to the
                // undo_stack:
                if random_between(0, 2, &mut seed) == 0 {
                    operations.push(DbOperation::Undo);
                } else {
                    undo_stack.push(DbOperation::Undo);
                }
            }
        } else {
            undo_stack.push(DbOperation::Undo);
        }
    }

    // Go through the items in the undo stack:
    let mut further_operations = vec![];
    let mut further_undo_stack = vec![];
    let mut consecutive_undos = 0;
    while let Some(_) = undo_stack.pop() {
        // Add the undo to the list of further operations to perform:
        further_operations.push(DbOperation::Undo);
        consecutive_undos += 1;
        // Randomly add a number of redos as well, and then undo them all:
        if random_between(0, 2, &mut seed) == 0 {
            let mut num_to_redo = random_between(1, consecutive_undos + 1, &mut seed);
            tracing::debug!("Redoing {num_to_redo} of {consecutive_undos} undos");
            let mut num_to_undo = 0;
            while num_to_redo > 0 {
                further_operations.push(DbOperation::Redo);
                num_to_redo -= 1;
                num_to_undo += 1;
            }
            while num_to_undo > 0 {
                further_operations.push(DbOperation::Undo);
                num_to_undo -= 1;
            }
            consecutive_undos = 0;
        }
    }

    operations.append(&mut further_operations);
    // Since further_undo_stack is a stack, we need to reverse it:
    further_undo_stack.reverse();
    operations.append(&mut further_undo_stack);

    println!(
        "{}",
        operations
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>()
            .join(match cli.vertical {
                true => "\n",
                false => " ",
            })
    );
}

pub async fn build_egg_demo(cli: &Cli) -> Relatable {
    tracing::debug!("build_egg_demo({cli:?})");

    let rltbl = Relatable::connect(Some(cli.database.as_str()))
        .await
        .expect("Error building demo");

    if let None = rltbl
        .connection
        .query_value("SELECT * FROM \"table\" WHERE \"table\" = 'penguin'", None)
        .await
        .unwrap()
    {
        panic!("Build a demonstration database first");
    }

    if let DbKind::Postgres = rltbl.connection.kind() {
        rltbl
            .connection
            .query(r#"DROP TABLE IF EXISTS "study" CASCADE"#, None)
            .await
            .expect("Error dropping study");
        rltbl
            .connection
            .query(r#"DROP TABLE IF EXISTS "egg" CASCADE"#, None)
            .await
            .expect("Error dropping egg");
    }

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    let sql = r#"INSERT INTO "table" ("table", "path") VALUES ('study', 'study.tsv')"#;
    rltbl.connection.query(sql, None).await.unwrap();

    // Create the study table.
    let sql = format!(
        r#"CREATE TABLE study (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             study_name TEXT UNIQUE,
             description TEXT
        )"#
    );
    rltbl.connection.query(&sql, None).await.unwrap();

    let sql = r#"INSERT INTO study VALUES
        (0, 0, 'FAKE123', 'Fake Study 123')"#;
    rltbl.connection.query(&sql, None).await.unwrap();

    let sql = r#"INSERT INTO "table" ("table", "path") VALUES ('egg', 'egg.tsv')"#;
    rltbl.connection.query(&sql, None).await.unwrap();

    // Create the egg table.
    let sql = format!(
        r#"CREATE TABLE egg (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             egg_id TEXT UNIQUE,
             individual_id TEXT
           )"#
    );
    rltbl.connection.query(&sql, None).await.unwrap();

    let sql = r#"INSERT INTO egg VALUES
        (0, 0, 'E1', 'N1')"#;
    rltbl.connection.query(&sql, None).await.unwrap();
    rltbl
}

/// TODO: Add a docstring and then move this to web.rs
async fn create_subquery(rltbl: &Relatable, tableset_name: &str, select: &Select) -> Select {
    let mut tables = HashSet::new();
    tables.insert(json!(select.table_name));
    for filter in &select.filters {
        let (t, _, _, _) = filter.parts();
        if t != "" {
            tables.insert(json!(t));
        }
    }

    if tables.len() == 1 {
        return select.clone();
    }

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    let sql = r#"INSERT INTO "table" ("table", "path") VALUES ('tableset', 'tableset.tsv')"#;
    rltbl.connection.query(sql, None).await.unwrap();

    if let DbKind::Postgres = rltbl.connection.kind() {
        rltbl
            .connection
            .query(r#"DROP TABLE IF EXISTS "tableset" CASCADE"#, None)
            .await
            .expect("Error dropping tableset");
    }

    let sql = format!(
        r#"CREATE TABLE tableset (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             tableset TEXT,
             "table" TEXT,
             "distinct" TEXT,
             "using" TEXT
           )"#,
    );
    rltbl.connection.query(&sql, None).await.unwrap();

    let sql = r#"INSERT INTO "tableset" VALUES
      (1, 1000, 'combined', 'study', 'study_name', NULL),
      (2, 2000, 'combined', 'penguin', 'individual_id', 'study_name'),
      (3, 3000, 'combined', 'egg', 'egg_id', 'individual_id')
    "#;
    rltbl.connection.query(sql, None).await.unwrap();

    let tables: Vec<serde_json::Value> = tables.into_iter().collect();
    let mut sql_param = SqlParam::new(&rltbl.connection.kind());
    let mut values = vec![];
    let (placeholder_list_1, mut these_values) = render_values(&tables, &mut sql_param).unwrap();
    values.append(&mut these_values);
    let (placeholder_list_2, mut these_values) = render_values(&tables, &mut sql_param).unwrap();
    values.append(&mut these_values);
    let (placeholder_list_3, mut these_values) = render_values(&tables, &mut sql_param).unwrap();
    values.append(&mut these_values);

    let sql = format!(
        r#"WITH RECURSIVE ancestors("table", "using") AS (
             SELECT "table", "using"
             FROM tableset
             WHERE "table" IN {placeholder_list_1}
             UNION
             SELECT tableset."table", tableset."using"
             FROM ancestors
             JOIN tableset ON ancestors."using" = tableset."distinct"
             WHERE tableset.tableset = '{tableset_name}'
           )
           SELECT tableset.*
           FROM tableset
           JOIN ancestors USING ("table")
           WHERE _order >= (SELECT MIN(_order) FROM tableset WHERE "table" IN {placeholder_list_2})
             AND _order <= (SELECT MAX(_order) FROM tableset WHERE "table" IN {placeholder_list_3})
           ORDER BY _order"#
    );
    let json_rows = rltbl
        .connection
        .query(&sql, Some(&json!(values)))
        .await
        .unwrap();

    let limit = select.limit;
    let mut sel = select.clone();
    let table_name = select.table_name.clone();
    let mut pkey = String::new();
    for json_row in json_rows.iter() {
        if table_name == json_row.get_string("table").unwrap() {
            pkey = json_row.get_string("distinct").unwrap();
        }
    }
    sel.select_column(&pkey);
    if table_name == json_rows.last().unwrap().get_string("table").unwrap() {
        sel = sel.order_by(&pkey);
    } else {
        sel.limit = 0;
    }
    let json_row = json_rows.first().unwrap();

    sel.table_name = json_row.get_string("table").unwrap();
    for json_row in json_rows.iter().skip(1) {
        sel.left_join(
            &mut sel.table_name.clone(),
            &json_row.get_string("using").unwrap(),
            &json_row.get_string("table").unwrap(),
            &json_row.get_string("using").unwrap(),
        );
    }

    Select {
        table_name,
        filters: vec![Filter::InSubquery {
            table: String::new(),
            column: pkey.clone(),
            subquery: sel.clone(),
        }],
        limit,
        ..Default::default()
    }
}

#[async_std::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing using --verbose flags
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(cli.verbose.tracing_level())
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    tracing::debug!("CLI {cli:?}");

    match &cli.command {
        Command::GenerateSeq {
            table,
            min_length,
            max_length,
        } => {
            let rltbl = Relatable::connect(Some(&cli.database))
                .await
                .expect("Could not connect to relatable database");
            generate_operation_sequence(&cli, &rltbl, table, *min_length, *max_length).await;
        }
        Command::SelectTest {
            table1,
            table2,
            column,
            value,
        } => {
            let rltbl = build_egg_demo(&cli).await;

            // Test subqueries:
            {
                let select = Select {
                    table_name: table1.to_string(),
                    filters: vec![Filter::Equal {
                        table: table2.to_string(),
                        column: column.to_string(),
                        value: serde_json::to_value(value).expect("Error parsing value"),
                    }],
                    ..Default::default()
                };
                let select_1 = create_subquery(&rltbl, table1, &select).await;

                let (sql, params) = select_1.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_1): {sql} {params:?}\n");

                let (sql, params) = select_1.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_1): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_1).await.unwrap();
                println!("ROWS (SELECT_1): {:#?}", result_set.rows);

                let count = rltbl.count(&select_1).await.unwrap();
                println!("COUNT (SELECT_1): {count}\n");
            }

            // Test select fields:
            {
                let mut query_params = QueryParams::new();
                query_params.insert("sample_number".to_string(), "eq.5".to_string());
                let select_2 = Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);

                let (sql, params) = select_2.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_2): {sql} {params:?}\n");

                let (sql, params) = select_2.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_2): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_2).await.unwrap();
                println!("ROWS (SELECT_2): {:#?}", result_set.rows);

                let count = rltbl.count(&select_2).await.unwrap();
                println!("COUNT (SELECT_2): {count}\n");
            }
            {
                let mut query_params = QueryParams::new();
                query_params.insert("sample_number".to_string(), "eq.9".to_string());
                let mut select_3 =
                    Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);
                select_3.select_column("species");

                let (sql, params) = select_3.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_3): {sql} {params:?}\n");

                let (sql, params) = select_3.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_3): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_3).await.unwrap();
                println!("ROWS (SELECT_3): {:#?}", result_set.rows);

                let count = rltbl.count(&select_3).await.unwrap();
                println!("COUNT (SELECT_3): {count}\n");
            }
            {
                let mut query_params = QueryParams::new();
                query_params.insert("island".to_string(), "eq.Biscoe".to_string());
                let mut select_4 =
                    Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);
                select_4.select_table_columns("penguin", &vec!["species", "island"]);
                select_4.select_columns(&vec!["study_name", "body_mass"]);

                let (sql, params) = select_4.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_4): {sql} {params:?}\n");

                let (sql, params) = select_4.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_4): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_4).await.unwrap();
                println!("ROWS (SELECT_4): {:#?}", result_set.rows);

                let count = rltbl.count(&select_4).await.unwrap();
                println!("COUNT (SELECT_4): {count}\n");
            }

            {
                let mut query_params = QueryParams::new();
                query_params.insert("sample_number".to_string(), "eq.9".to_string());
                let mut select_5 =
                    Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);
                select_5.select_alias("penguin", "island", "location");

                let (sql, params) = select_5.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_5): {sql} {params:?}\n");

                let (sql, params) = select_5.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_5): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_5).await.unwrap();
                println!("ROWS (SELECT_5): {:#?}", result_set.rows);

                let count = rltbl.count(&select_5).await.unwrap();
                println!("COUNT (SELECT_5): {count}\n");
            }

            {
                let mut query_params = QueryParams::new();
                query_params.insert("sample_number".to_string(), "eq.9".to_string());
                let mut select_6 =
                    Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);
                select_6
                    .select_expression("CASE WHEN island = 'Biscoe' THEN 'BISCOE' END", "location");

                let (sql, params) = select_6.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_6): {sql} {params:?}\n");

                let (sql, params) = select_6.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_6): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_6).await.unwrap();
                println!("ROWS (SELECT_6): {:#?}", result_set.rows);

                let count = rltbl.count(&select_6).await.unwrap();
                println!("COUNT (SELECT_6): {count}\n");
            }

            {
                let mut query_params = QueryParams::new();
                query_params.insert("sample_number".to_string(), "eq.9".to_string());
                let mut select_7 =
                    Select::from_path_and_query(&rltbl, "penguin.tsv", &query_params);
                select_7.select_all(&rltbl, "penguin").await.unwrap();

                let (sql, params) = select_7.to_sql(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL (SELECT_7): {sql} {params:?}\n");

                let (sql, params) = select_7.to_sql_count(&rltbl.connection.kind()).unwrap();
                println!("TO_SQL_COUNT (SELECT_7): {sql} {params:?}\n");

                let result_set = rltbl.fetch(&select_7).await.unwrap();
                println!("ROWS (SELECT_7): {:#?}", result_set.rows);

                let count = rltbl.count(&select_7).await.unwrap();
                println!("COUNT (SELECT_7): {count}\n");
            }
        }
    }
}
