mod scs;

use crate::{
    cli::scs::{ScsArgs, handle_scs_command},
    event::{Event, EventBus},
};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use tokio::{
    select, signal, spawn,
    task::{JoinError, JoinHandle},
};

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
    let event_bus = EventBus::new(100);
    let event_bus_clone = event_bus.clone();
    let signal_handle: JoinHandle<Result<()>> = spawn(async move {
        signal::ctrl_c()
            .await
            .context("Failed to listen for Ctrl+C signal")?;
        event_bus_clone
            .publish(Event::Shutdown)
            .context("Failed to send shutdown event")?;
        Ok(())
    });
    let main_task = match &cli.command {
        Commands::Scs(args) => handle_scs_command(args, event_bus),
    };
    select! {
        main_res = main_task => {
            main_res?;
        }
        signal_res = signal_handle => {
            handle_signal_result(signal_res)?;
        }
    }
    Ok(())
}

fn handle_signal_result(signal_res: Result<Result<()>, JoinError>) -> Result<()> {
    match signal_res {
        Ok(Ok(())) => Ok(()),
        // An error occurred in the signal handler itself (e.g., the handler could not be registered)
        Ok(Err(e)) => Err(anyhow!(e)),
        Err(join_err) => {
            if join_err.is_panic() {
                Err(anyhow!(join_err))
            } else if join_err.is_cancelled() {
                Err(anyhow!(join_err))
            } else {
                Err(anyhow!(join_err))
            }
        }
    }
}
