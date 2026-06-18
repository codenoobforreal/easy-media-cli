mod cli;
mod common;
mod domain;
mod ffmpeg_progress;
mod infra;
mod media_metadata;
mod task;
mod tasks;
mod ui;

use anyhow::Result;
use cli::run_cli;

fn main() -> Result<()> {
    run_cli()?;
    Ok(())
}
