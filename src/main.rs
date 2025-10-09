//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl).

use anyhow::Result;
use rltbl::cli;

#[tokio::main]
async fn main() -> Result<()> {
    cli::process_command().await;
    Ok(())
}
