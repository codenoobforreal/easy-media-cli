use crate::{
    cli::GlobalConfig,
    domain::{event::EventBus, media::MetadataFetcher},
    infra::{CapturingCommandRunner, FileSystem},
    task::{
        command::CommandTaskWrapper,
        execution::{build_task_list, run_tasks_with_ui},
    },
    tasks::thumbnail_generator::ThumbnailGenerator,
    ui::renderer::Renderer,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, value_parser};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Args, Debug)]
pub struct ScsArgs {
    /// Path to an input video file or a directory. Directories are processed recursively
    #[arg(short, long)]
    input: PathBuf,
    /// Directory for output files. If not set, a subfolder named after the input video is automatically created alongside it
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Maximum recursion depth for directory scans (0–10). If not given, only the specified directory is processed (equivalent to depth 0)
    #[arg(short, long, value_parser = value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
    /// Scene detection sensitivity threshold (0.01 – 1.0)
    #[arg(short, long, default_value_t = 0.3, value_parser = parse_threshold)]
    threshold: f32,
    /// Thumbnail width in pixels (minimum 1). Height is scaled proportionally
    #[arg(short, long, value_parser = value_parser!(u16).range(1..))]
    width: Option<u16>,
}

fn parse_threshold(s: &str) -> Result<f32> {
    let val = s
        .parse::<f32>()
        .map_err(|_| anyhow!("Invalid float: '{s}'"))?;
    if !(0.01..=1.0).contains(&val) {
        bail!("Scene detection sensitivity threshold must be between 0.01 and 1.0, got {val}");
    }

    Ok(val)
}

pub fn handle_scene_cut_snap(
    args: &ScsArgs,
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
            let generator = ThumbnailGenerator::new(
                task_id,
                video,
                args.output.as_deref(),
                args.threshold,
                args.width,
                &metadata,
            )?;
            let wrapped = CommandTaskWrapper::new(
                generator,
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
