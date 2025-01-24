//! # rltbl/relatable
//!
//! This is relatable.

use anyhow::Result;
use rltbl::cli;

#[async_std::main]
async fn main() -> Result<()> {
    cli::process_command().await;
    Ok(())
}
