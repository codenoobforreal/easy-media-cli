use crate::{
    cli::GlobalConfig,
    domain::{
        event::EventBus,
        media::{MetadataFetcher, Resolution},
    },
    infra::{CapturingCommandRunner, FileSystem},
    task::{
        command::CommandTaskWrapper,
        execution::{build_task_list, run_tasks_with_ui},
    },
    tasks::video_encoder::VideoEncoder,
    ui::renderer::Renderer,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, value_parser};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Args, Debug)]
pub struct EvArgs {
    /// Path to an input video file or a directory. Directories are processed recursively
    #[arg(short, long)]
    input: PathBuf,
    /// Directory for output files. When input is a file, defaults to that file's parent directory
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Maximum recursion depth for directory scans (0–10). If not given, only the specified directory is processed (equivalent to depth 0)
    #[arg(short, long, value_parser = value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
    /// Maximum output resolution, specified as WIDTHxHEIGHT (e.g. 1280x720). If the source resolution exceeds this value, the output is capped. Defaults to 1920x1080 when omitted
    #[arg(short, long)]
    resolution: Option<Resolution>,
    /// Maximum output frame rate in frames per second (1–120). If the source exceeds this, it is capped
    #[arg(short, long, default_value_t = 24.0,value_parser = parse_fps)]
    fps: f64,
}

fn parse_fps(s: &str) -> Result<f64> {
    let val = s
        .parse::<f64>()
        .map_err(|_| anyhow!("Invalid float: '{s}'"))?;
    if !(0.01..=120.0).contains(&val) {
        bail!("Maximum fps must be between 1.0 and 120.0, got {val}");
    }

    Ok(val)
}

pub fn handle_encode_video(
    args: &EvArgs,
    event_bus: Arc<dyn EventBus>,
    command_runner: Arc<dyn CapturingCommandRunner>,
    metadata_fetcher: Arc<dyn MetadataFetcher>,
    file_system: Arc<dyn FileSystem>,
    renderer: Box<dyn Renderer>,
    global_config: &GlobalConfig,
) -> Result<()> {
    let render_interval = Duration::from_millis(global_config.render_interval_ms);
    let progress_threshold = global_config.progress_threshold;
    let fs_clone = file_system.clone();

    let tasks = build_task_list(
        &args.input,
        args.depth,
        file_system.as_ref(),
        move |task_id, video| {
            let metadata = metadata_fetcher.fetch_metadata(&video)?;
            let encoder = VideoEncoder::new(
                task_id,
                video,
                args.output.as_deref(),
                args.resolution,
                args.fps,
                &metadata,
            )?;
            let wrapped = CommandTaskWrapper::new(
                encoder,
                command_runner.clone(),
                fs_clone.clone(),
                render_interval,
                progress_threshold,
            );
            Ok(Arc::new(wrapped))
        },
    )?;

    drop(file_system);

    run_tasks_with_ui(tasks, event_bus, renderer, render_interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands, EvArgs};
    use anyhow::Context;
    use clap::Parser;
    use insta::assert_debug_snapshot;

    fn parse_ve_args(cmd: &[&str]) -> Result<EvArgs> {
        let cli = Cli::try_parse_from(cmd)
            .with_context(|| format!("Failed to parse CLI args: {cmd:?}"))?;
        match cli.command {
            Commands::Ev(args) => Ok(args),
            Commands::Scs(..) => panic!("parse_ve_args only supports Ve subcommand"),
        }
    }

    #[test]
    fn default_values_are_correct() {
        let args = parse_ve_args(&["easy-media-cli", "ev", "-i", "test.mp4"]).unwrap();
        assert_debug_snapshot!(args, @r#"
            EvArgs {
                input: "test.mp4",
                output: None,
                depth: None,
                resolution: None,
                fps: 24.0,
            }
            "#);
    }

    #[test]
    fn fps_below_1_rejected() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "-i", "test.mp4", "-f", "0.0"])
            .unwrap_err();
        assert_debug_snapshot!(err.kind(), @"ValueValidation");
    }

    #[test]
    fn fps_above_120_rejected() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "-i", "test.mp4", "-f", "121.0"])
            .unwrap_err();
        assert_debug_snapshot!(err.kind(), @"ValueValidation");
    }

    #[test]
    fn depth_above_10_rejected() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "-i", "test.mp4", "-d", "11"])
            .unwrap_err();
        assert_debug_snapshot!(err.kind(), @"ValueValidation");
    }

    #[test]
    fn invalid_resolution_format_rejected() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "-i", "test.mp4", "-r", "invalid"])
            .unwrap_err();
        assert_debug_snapshot!(err.kind(), @"ValueValidation");
    }

    #[test]
    fn resolution_zero_dimension_rejected() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "-i", "test.mp4", "-r", "0x1080"])
            .unwrap_err();
        assert_debug_snapshot!(err.kind(), @"ValueValidation");
    }

    #[test]
    fn missing_input_returns_missing_arg_error() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev"]).unwrap_err();
        assert_debug_snapshot!(err.kind(), @"MissingRequiredArgument");
    }

    #[test]
    fn ve_help_text_stable_snapshot() {
        let err = Cli::try_parse_from(["easy-media-cli", "ev", "--help"]).unwrap_err();
        assert_debug_snapshot!(err.kind(), @"DisplayHelp");
    }
}
