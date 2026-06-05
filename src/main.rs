mod cli;
mod common;
mod error;
mod event;
mod executor;
mod metadata;
mod task;
mod ui;

use anyhow::Result;
use cli::run_cli;

#[tokio::main]
async fn main() -> Result<()> {
    run_cli().await?;
    Ok(())
}
