//! API tests

use rltbl::{
    core::{Change, ChangeAction, ChangeSet, Relatable, RLTBL_DEFAULT_DB},
    select::Select,
    sql::{CachingStrategy, JsonRow},
};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use rand::{
    distributions::{Distribution as _, Uniform},
    rngs::StdRng,
    SeedableRng as _,
};
use serde_json::json;
use std::{
    thread,
    time::{Duration, Instant},
};

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
    user: Option<String>,

    #[command(flatten)]
    verbose: Verbosity,

    #[arg(long, action = ArgAction::SetTrue)]
    vertical: bool,

    #[arg(long, action = ArgAction::Set)]
    seed: Option<u64>,

    /// One of: none, truncate, truncate_all, trigger, memory
    #[arg(long, default_value = "trigger", action = ArgAction::Set)]
    caching: CachingStrategy,

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
    /// Test database read performance by repeatedly counting the number of rows in a given
    /// table.
    TestReadPerf {
        #[arg(action = ArgAction::Set)]
        table_size: usize,

        #[arg(action = ArgAction::Set)]
        fetches: usize,

        #[arg(action = ArgAction::Set)]
        edit_rate: usize,

        #[arg(action = ArgAction::Set)]
        fail_after_secs: u64,

        /// Overwrite an existing database
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
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
            let rltbl = Relatable::connect(Some(&cli.database), &cli.caching)
                .await
                .expect("Could not connect to relatable database");
            generate_operation_sequence(&cli, &rltbl, table, *min_length, *max_length).await;
        }
        Command::TestReadPerf {
            table_size,
            fetches,
            edit_rate,
            fail_after_secs,
            force,
        } => {
            tracing::info!("Building demonstration database with {table_size} rows per table ...");
            let rltbl =
                Relatable::build_demo(Some(&cli.database), force, *table_size, &cli.caching)
                    .await
                    .unwrap();
            let tables_to_choose_from = vec!["penguin", "qenguin", "renguin", "senguin"];
            for table in tables_to_choose_from.iter() {
                if *table != "penguin" {
                    rltbl
                        .create_demo_table(table, force, *table_size)
                        .await
                        .unwrap();
                }
            }
            tracing::info!("Demonstration database built and loaded.");

            fn random_op<'a>() -> &'a str {
                match random_between(0, 3, &mut -1) {
                    0 => "add",
                    1 => "update",
                    2 => "move",
                    _ => unreachable!(),
                }
            }

            fn random_table<'a>(tables_to_choose_from: &'a Vec<&str>) -> &'a str {
                match random_between(0, 4, &mut -1) {
                    0 => tables_to_choose_from[0],
                    1 => tables_to_choose_from[1],
                    2 => tables_to_choose_from[2],
                    3 => tables_to_choose_from[3],
                    _ => unreachable!(),
                }
            }

            tracing::info!("Counting rows from tables {tables_to_choose_from:?} ...");
            let now = Instant::now();
            let mut i = 0;
            let mut elapsed;
            let table_to_edit = tables_to_choose_from[0];
            while i < *fetches {
                let table = random_table(&tables_to_choose_from);
                let join_table = {
                    let mut join_table = random_table(&tables_to_choose_from);
                    while join_table == table {
                        join_table = random_table(&tables_to_choose_from);
                    }
                    join_table
                };
                let mut select = Select::from(table);
                select.left_join(table, "individual_id", join_table, "individual_id");

                let count = rltbl.count(&select).await.unwrap();
                tracing::debug!("Counted {count} rows from table '{table}'");
                elapsed = now.elapsed().as_secs();
                if elapsed > *fail_after_secs {
                    panic!("Taking longer than {fail_after_secs}s. Timing out.");
                }
                if *edit_rate != 0 && random_between(0, *edit_rate, &mut -1) == 1 {
                    let user = match &cli.user {
                        Some(user) => user.clone(),
                        None => whoami::username(),
                    };
                    let table = table_to_edit;
                    match random_op() {
                        "add" => {
                            let after_id = random_between(1, *table_size, &mut -1);
                            let row = rltbl
                                .add_row(table, &user, Some(after_id), &JsonRow::new())
                                .await
                                .unwrap();
                            tracing::info!("Added row {} (order {}) to {table}", row.id, row.order);
                        }
                        "update" => {
                            let row_to_update = random_between(1, *table_size, &mut -1);
                            let num_changes = rltbl
                                .set_values(&ChangeSet {
                                    user,
                                    action: ChangeAction::Do,
                                    table: table.to_string(),
                                    description: "Set one value".to_string(),
                                    changes: vec![Change::Update {
                                        row: row_to_update,
                                        column: "study_name".to_string(),
                                        before: json!("FAKE123"),
                                        after: json!("PHONY123"),
                                    }],
                                })
                                .await
                                .unwrap()
                                .changes
                                .len();
                            if num_changes < 1 {
                                panic!("No changes made to {table}");
                            }
                            tracing::info!("Updated row {row_to_update} in {table}");
                        }
                        "move" => {
                            let after_id = random_between(1, *table_size, &mut -1);
                            let row = random_between(1, *table_size, &mut -1);
                            let new_order = rltbl
                                .move_row(table, &user, row, after_id)
                                .await
                                .expect("Failed to move row within {table}");
                            if new_order > 0 {
                                tracing::info!("Moved row {row} after row {after_id} in {table}");
                            } else {
                                panic!("No changes made to {table}");
                            }
                        }
                        operation => panic!("Unrecognized operation: {operation}"),
                    }
                } else {
                    tracing::debug!("Not making any edits to {table}");
                }

                // A small sleep to prevent over-taxing the CPU:
                thread::sleep(Duration::from_millis(2));
                i += 1;
            }
            elapsed = now.elapsed().as_secs();
            tracing::info!(
                "Performed {fetches} counts using strategy {} on tables {tables_to_choose_from:?} \
                 in {elapsed}s",
                cli.caching
            );
        }
    }
}
