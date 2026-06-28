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
use anyhow::Result;
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
    #[arg(short, long, default_value_t = 24,value_parser = value_parser!(u8).range(1..=120))]
    fps: u8,
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
