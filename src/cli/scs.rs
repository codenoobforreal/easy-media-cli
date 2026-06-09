use crate::{
    common::collect_videos,
    event::EventBus,
    executor::Executor,
    task::{SharedTask, Thumbnail},
    ui::Ui,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, value_parser};
use std::{fs, path::PathBuf, sync::Arc};
use tokio::{join, spawn};

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
    /// concurrency
    #[arg(short, long, default_value_t = 1,value_parser = value_parser!(u8).range(1..))]
    concurrency: u8,
}

fn validate_threshold(s: f32) -> Result<f32> {
    if s <= 0.0 || s > 1.0 {
        bail!("The threshold must be greater than 0.0 and not exceed 1.0");
    }
    Ok(s)
}

pub async fn handle_scs_command(args: &ScsArgs, event_bus: EventBus) -> Result<()> {
    let threshold = validate_threshold(args.threshold)?;
    let depth = args.depth.unwrap_or_default();
    let videos = collect_videos(&args.input, depth);
    if videos.len() == 0 {
        bail!("no video found in path:\n{}", args.input.display())
    }
    fs::create_dir_all(&args.output)?;
    let mut tasks: Vec<SharedTask> = Vec::with_capacity(videos.len());
    let mut task_id_counter = 1u64;
    for video in videos {
        tasks.push(Arc::new(Thumbnail::new(
            task_id_counter,
            video,
            args.output.clone(),
            threshold,
            args.width,
        )));
        task_id_counter += 1;
    }
    let executor = Executor::new(args.concurrency as usize, event_bus.clone());
    let mut ui = Ui::new(executor.clone(), event_bus.subscribe());
    let executor_clone = executor.clone();
    let tasks_clone = tasks.clone();
    executor.start_event_listener().await;
    let run_all_handle = spawn(async move { executor_clone.run_all(tasks_clone).await });
    let (run_all_res, ui_res) = join!(run_all_handle, ui.run());
    let mut errors = vec![];
    match run_all_res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => errors.push(e),
        Err(_) => errors.push(anyhow!("Running all task panicked or cancelled")),
    }
    match ui_res {
        Ok(_) => {}
        Err(e) => errors.push(e),
    }
    if !errors.is_empty() {
        let mut main_err = anyhow!("Scene cut snap failed with {} errors", errors.len());
        for err in errors {
            main_err = main_err.context(err);
        }
        return Err(main_err);
    }
    Ok(())
}
