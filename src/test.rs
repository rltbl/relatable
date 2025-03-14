//! API tests

use rltbl::core::{Relatable, RLTBL_DEFAULT_DB};

use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

use rand::{
    distributions::{Distribution as _, Uniform},
    rngs::StdRng,
    SeedableRng as _,
};

#[derive(Parser, Debug)]
#[command(version, about = "Relatable (rltbl): Connect your data!", long_about = None)]
pub struct Cli {
    /// Location of the database.
    #[arg(long,
          default_value = RLTBL_DEFAULT_DB,
          action = ArgAction::Set,
          env = "RLTBL_DATABASE")]
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
    }
}
