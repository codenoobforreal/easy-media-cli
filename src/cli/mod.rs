mod scs;

use crate::{
    domain::Event,
    infra::{
        CapturingCommandRunner, DefaultCommandRunner, DefaultEventBus, DefaultFileSystem, EventBus,
        FileSystem,
    },
    media_metadata::{DefaultMetadataFetcher, MetadataFetcher},
};
use anyhow::Result;
use clap::{Parser, Subcommand};
pub use scs::{ScsArgs, handle_scs_command};
use std::{
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Parser, Debug)]
#[command(version, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[command(about, long_about = None)]
pub enum Commands {
    /// FFmpeg scene detection batch thumbnail generator
    #[allow(clippy::doc_markdown)]
    Scs(ScsArgs),
}

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let event_bus: Arc<dyn EventBus> = Arc::new(DefaultEventBus::default());
    let file_system: Arc<dyn FileSystem> = Arc::new(DefaultFileSystem);
    let command_runner: Arc<dyn CapturingCommandRunner> = Arc::new(DefaultCommandRunner);
    let metadata_fetcher: Arc<dyn MetadataFetcher> =
        Arc::new(DefaultMetadataFetcher::new(command_runner.clone()));

    // 全局注册 Ctrl+C 监听：收到信号后向事件总线发布 Shutdown
    let is_first_cancel = Arc::new(AtomicBool::new(true));
    let bus_for_signal = event_bus.clone();
    ctrlc::set_handler(move || {
        if is_first_cancel.load(Ordering::SeqCst) {
            // 第一次按下：优雅取消，等待任务收尾
            is_first_cancel.store(false, Ordering::SeqCst);
            let _ = bus_for_signal.publish(Event::Shutdown);
        } else {
            // 第二次按下：强制退出
            process::exit(1);
        }
    })?;

    match &cli.command {
        Commands::Scs(args) => handle_scs_command(
            args,
            event_bus,
            &command_runner,
            &metadata_fetcher,
            &file_system,
        )?,
    }

    Ok(())
}
