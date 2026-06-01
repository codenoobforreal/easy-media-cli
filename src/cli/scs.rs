use crate::{
    common::collect_videos,
    task::{
        IdGenerator, Manager, Metadata, MetadataMap, NotifyEvent, RegistryMap, SceneCutSnapTask,
        Task,
    },
    ui::Renderer,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, value_parser};
use std::{collections::HashMap, ffi::OsString, fs, path::PathBuf, sync::mpsc};

#[derive(Args, Debug)]
pub struct ScsArgs {
    /// Video file or folder
    #[arg(short, long)]
    input: PathBuf,
    /// Dest folder path
    #[arg(short, long)]
    output: PathBuf,
    /// Scene detect threshold
    #[arg(short, long, default_value_t = 0.3)]
    threshold: f32,
    /// Final generated image width
    #[arg(short, long, value_parser = value_parser!(u16).range(1..))]
    width: Option<u16>,
    /// Traversal depth
    #[arg(short, long, value_parser = value_parser!(u8).range(0..))]
    depth: Option<u8>,
}

fn validate_threshold(s: f32) -> Result<f32> {
    if s <= 0.0 || s > 1.0 {
        bail!("The threshold must be greater than 0.0 and not exceed 1.0");
    }

    Ok(s)
}

fn create_tasks<I, P>(
    videos: I,
    output: PathBuf,
    threshold: f32,
    width: Option<u16>,
) -> Result<(RegistryMap, MetadataMap)>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let videos: Vec<PathBuf> = videos.into_iter().map(Into::into).collect();
    let total = videos.len();

    let mut tasks = RegistryMap::with_capacity(total);
    let mut metadata_map = HashMap::with_capacity(total);

    let mut id_gen = IdGenerator::new();
    for video in videos {
        let task_id = id_gen.next();
        let task = Box::new(SceneCutSnapTask::new(
            video,
            output.clone(),
            threshold,
            width,
        ));
        metadata_map.insert(
            task_id,
            Metadata::new(
                OsString::from(task.file_name().ok_or_else(|| {
                    anyhow!(
                        "Failed to extract file name from input: {}",
                        task.input().display()
                    )
                })?),
                task.task_type(),
                task.supports_progress(),
            ),
        );

        tasks.insert(task_id, task);
    }

    Ok((tasks, metadata_map))
}

pub fn handle_scs_command(args: &ScsArgs) -> Result<()> {
    let threshold = validate_threshold(args.threshold)?;
    let depth = args.depth.unwrap_or_default();
    let videos = collect_videos(&args.input, depth)?;

    if videos.len() == 0 {
        bail!("no video found in path:\n{}", args.input.display())
    }

    fs::create_dir_all(&args.output)?;

    let (sender, receiver) = mpsc::channel::<NotifyEvent>();
    let (tasks, metadata) = create_tasks(videos, args.output.clone(), threshold, args.width)?;

    let run_handle = Manager::run_serially(tasks, sender);
    let mut renderer = Renderer::new(metadata, receiver);
    renderer.run()?;

    match run_handle.join() {
        Ok(task_result) => task_result,
        Err(panic_box) => {
            let panic_info = if let Some(msg) = panic_box.downcast_ref::<&str>() {
                msg.to_string()
            } else if let Some(msg) = panic_box.downcast_ref::<String>() {
                msg.clone()
            } else {
                format!(
                    "Unknown panic; unable to resolve exception type: {:?}",
                    panic_box.type_id()
                )
            };
            Err(anyhow!(
                "A panic occurred in the task thread: {}",
                panic_info
            ))?
        }
    }

    Ok(())
}
