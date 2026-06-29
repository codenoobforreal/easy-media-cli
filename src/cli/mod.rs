//! cli 入口层
//! 程序入口、参数解析
//! 调用任务层能力

pub mod encode_video;
pub mod scene_cut_snap;

use crate::{
    domain::{event::EventBus, media::MetadataFetcher},
    infra::{
        CapturingCommandRunner, DefaultCommandRunner, DefaultFileSystem, DefaultMetadataFetcher,
        FileSystem,
    },
    ui::renderer::DefaultRenderer,
};
use anyhow::{Result, anyhow, bail};
use clap::{Parser, Subcommand, value_parser};
pub use encode_video::{EvArgs, handle_encode_video};
pub use scene_cut_snap::{ScsArgs, handle_scene_cut_snap};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(version, propagate_version = true)]
pub struct Cli {
    #[command(flatten)]
    global: GlobalConfig,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Parser)]
pub struct GlobalConfig {
    /// Terminal render interval in milliseconds
    #[arg(long, global = true, default_value_t = 100, value_parser = value_parser!(u64).range(1..=10000))]
    pub render_interval_ms: u64,

    /// Minimum percentage change to trigger a progress update
    #[arg(long, global = true, default_value_t = 1.0, value_parser = parse_progress_threshold)]
    pub progress_threshold: f32,
}

impl GlobalConfig {
    /// 返回解析的默认值而非 Default trait 的默认值（其不经过 clap）
    pub fn parser_default() -> Self {
        Self {
            render_interval_ms: 100,
            progress_threshold: 1.0,
        }
    }
}

fn parse_progress_threshold(s: &str) -> Result<f32> {
    let val = s
        .parse::<f32>()
        .map_err(|_| anyhow!("Invalid float: '{s}'"))?;
    if !(0.1..=10.0).contains(&val) {
        bail!("Progress threshold must be between 0.1 and 10.0, got {val}");
    }

    Ok(val)
}

#[derive(Subcommand, Debug)]
#[command(about)]
pub enum Commands {
    /// FFmpeg scene detection batch thumbnail generator
    #[allow(clippy::doc_markdown)]
    Scs(ScsArgs),
    /// Batch SVT-AV1 archival encoding with resolution/frame-rate caps
    Ev(EvArgs),
}

pub fn run_cli(event_bus: Arc<dyn EventBus>) -> Result<()> {
    let cli = Cli::parse();

    let file_system: Arc<dyn FileSystem> = Arc::new(DefaultFileSystem);
    let command_runner: Arc<dyn CapturingCommandRunner> = Arc::new(DefaultCommandRunner);
    let metadata_fetcher: Arc<dyn MetadataFetcher> =
        Arc::new(DefaultMetadataFetcher::new(command_runner.clone()));
    let terminal_renderer = Box::new(DefaultRenderer::default());

    match &cli.command {
        Commands::Scs(args) => handle_scene_cut_snap(
            args,
            event_bus,
            command_runner,
            metadata_fetcher,
            file_system,
            terminal_renderer,
            &cli.global,
        )?,
        Commands::Ev(args) => handle_encode_video(
            args,
            event_bus,
            command_runner,
            metadata_fetcher,
            file_system,
            terminal_renderer,
            &cli.global,
        )?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use insta::assert_debug_snapshot;

    mod cli_definition {
        use super::*;

        #[test]
        fn verify_cli_definition() {
            Cli::command().debug_assert();
        }
    }

    mod global_flags {
        use super::*;

        #[test]
        fn version_flag_works() {
            let err = Cli::try_parse_from(["easy-media-cli", "--version"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayVersion");
        }

        #[test]
        fn root_help_flag_works() {
            let err = Cli::try_parse_from(["easy-media-cli", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }

        #[test]
        fn unknown_subcommand_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "unknown"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"InvalidSubcommand");
        }

        #[test]
        fn no_subcommand_returns_error() {
            let err = Cli::try_parse_from(["easy-media-cli"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelpOnMissingArgumentOrSubcommand");
        }

        #[test]
        fn root_help_text_stable_snapshot() {
            let err = Cli::try_parse_from(["easy-media-cli", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }
    }
}
