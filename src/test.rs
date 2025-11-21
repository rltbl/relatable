//! API tests

use rltbl::{
    core::{Change, ChangeAction, ChangeSet, Relatable, RowID, RLTBL_DEFAULT_DB},
    select::Select,
    sql::CachingStrategy,
};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use rand::{distr::Uniform, rngs::StdRng, Rng, SeedableRng as _};
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

fn random_between(min: usize, max: usize, seed: &mut i64) -> usize {
    let mut rng = if *seed < 0 {
        StdRng::from_rng(&mut rand::rng())
    } else {
        *seed += 10;
        StdRng::seed_from_u64(*seed as u64)
    };
    rng.sample(Uniform::new(min, max).unwrap())
}

#[tokio::main]
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
        Command::TestReadPerf {
            table_size,
            fetches,
            edit_rate,
            fail_after_secs,
            force,
        } => {
            tracing::info!("Building demonstration database with {table_size} rows per table ...");
            let rltbl = Relatable::init(&true, Some(&cli.database), &cli.caching)
                .await
                .expect("Connect error");
            rltbl::demo::build_demo(&rltbl, force, *table_size)
                .await
                .expect("Error building demonstration database");
            let tables_to_choose_from = vec!["penguin", "qenguin", "renguin", "senguin"];
            for table in tables_to_choose_from.iter() {
                if *table != "penguin" {
                    rltbl::demo::create_penguin_table(&rltbl, Some(table), force, *table_size)
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
                            let after_id = random_between(1, *table_size, &mut -1) as RowID;
                            let row = rltbl
                                .add_row(
                                    table,
                                    &user,
                                    Some(after_id),
                                    &json!({"study_name": "FAKE123"}).as_object().unwrap(),
                                )
                                .await
                                .unwrap();
                            tracing::info!("Added row {} (order {}) to {table}", row.id, row.order);
                        }
                        "update" => {
                            let row_to_update = random_between(1, *table_size, &mut -1) as RowID;
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
                            let after_id = random_between(1, *table_size, &mut -1) as RowID;
                            let row = random_between(1, *table_size, &mut -1) as RowID;
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
