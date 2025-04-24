//! API tests

use rltbl::core::{Relatable, RLTBL_DEFAULT_DB};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use rand::{
    distributions::{Distribution as _, Uniform},
    rngs::StdRng,
    SeedableRng as _,
};
use serde_json::json;
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

/// TODO: Add a docstring and then move this to web.rs
async fn joined_query(
    rltbl: &rltbl::core::Relatable,
    tableset_name: &str,
    select: &rltbl::core::Select,
) -> rltbl::core::Select {
    let mut tables = HashSet::new();
    tables.insert(json!(select.table_name));
    for filter in &select.filters {
        let (t, _, _, _) = filter.parts();
        if t != "" {
            tables.insert(json!(t));
        }
    }

    tracing::info!("Tables: {tables:?}");

    if tables.len() == 1 {
        return select.clone();
    }

    let tables: Vec<serde_json::Value> = tables.into_iter().collect();
    let (placeholder_list, values) = rltbl::core::render_values(
        &tables,
        &mut rltbl::sql::SqlParam::new(&rltbl.connection.kind()),
    )
    .unwrap();

    let sql = format!(
        r#"WITH RECURSIVE ancestors("table", "using") AS (
             SELECT "table", "using"
             FROM tableset
             WHERE "table" IN {placeholder_list}
             UNION
             SELECT tableset."table", tableset."using"
             FROM ancestors
             JOIN tableset ON ancestors."using" = tableset."distinct"
             WHERE tableset.tableset = '{tableset_name}'
           )
           SELECT tableset.*
           FROM tableset
           JOIN ancestors USING ("table")
           WHERE _order >= (SELECT MIN(_order) FROM tableset WHERE "table" IN {placeholder_list})
             AND _order <= (SELECT MAX(_order) FROM tableset WHERE "table" IN {placeholder_list})
           ORDER BY _order"#
    );
    let json_rows = rltbl
        .connection
        .query(&sql, Some(&json!(values)))
        .await
        .unwrap();
    tracing::info!("TABLESET {json_rows:?}",);

    let limit = select.limit;
    let mut sel = select.clone();
    let table_name = select.table_name.clone();
    let mut pkey = String::new();
    for json_row in json_rows.iter() {
        if table_name == json_row.get_string("table").unwrap() {
            pkey = json_row.get_string("distinct").unwrap();
        }
    }
    sel.select = vec![pkey.clone()];
    if table_name == json_rows.last().unwrap().get_string("table").unwrap() {
        sel = sel.order_by(&pkey);
    } else {
        sel.limit = 0;
    }
    let json_row = json_rows.first().unwrap();

    sel.table_name = json_row.get_string("table").unwrap();
    for json_row in json_rows.iter().skip(1) {
        sel.left_join_using(
            &json_row.get_string("table").unwrap(),
            &json_row.get_string("using").unwrap(),
        );
    }

    rltbl::core::Select {
        table_name,
        filters: vec![rltbl::core::Filter::InSubquery {
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

            // TODO: Remove this later
            // if true {
            //     let select = rltbl::core::Select {
            //         table_name: "penguin".to_string(),
            //         view_name: "".to_string(),
            //         filters: vec![rltbl::core::Filter::Equal {
            //             table: "menguin".to_string(),
            //             column: "species".to_string(),
            //             value: serde_json::Value::String("Mygoscelis adeliae".to_string()),
            //         }],
            //         ..Default::default()
            //     };
            //     tracing::info!("SELECT {select:?}");

            // let joined_select = joined_query(&rltbl, table, &select).await;
            // tracing::info!("JOINED SELECT: {joined_select:?}");
            // let count = rltbl.count(&joined_select).await.unwrap();
            // tracing::info!("COUNT: {count}");
            // }

            generate_operation_sequence(&cli, &rltbl, table, *min_length, *max_length).await;
        }
    }
}
