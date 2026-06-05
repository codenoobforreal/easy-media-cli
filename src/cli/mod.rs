mod scs;

use crate::cli::scs::{ScsArgs, handle_scs_command};
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// An easy-to-use command-line tool for multimedia processing
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[command(about, long_about = None)]
enum Commands {
    /// Scene cut snap
    Scs(ScsArgs),
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Scs(args) => handle_scs_command(args).await?,
    }

    Ok(())
}
