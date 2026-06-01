mod cli;
mod client;
mod command_executor;
mod common;
mod metadata;
mod progress;
mod task;
mod ui;

use crate::cli::run_cli;
use anyhow::Result;

fn main() -> Result<()> {
    if let Err(e) = run_cli() {
        eprintln!("Error: {}", e.to_string());
        return Err(e);
    }
    Ok(())
}
