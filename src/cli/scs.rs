use crate::{
    common::collect_videos,
    event::EventBus,
    executor::Executor,
    task::{SharedTask, Thumbnail},
    ui::Ui,
};
use anyhow::{Result, bail};
use clap::{Args, value_parser};
use std::{fs, path::PathBuf, sync::Arc};

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

pub async fn handle_scs_command(args: &ScsArgs) -> Result<()> {
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
    let event_bus = EventBus::new(100);
    let executor = Executor::new(args.concurrency as usize, event_bus.clone());
    let mut ui = Ui::new(executor.clone(), event_bus.subscribe());
    let executor_clone = executor.clone();
    let tasks_clone = tasks.clone();
    executor.start_event_listener().await;
    tokio::spawn(async move {
        if let Err(e) = executor_clone.run_all(tasks_clone).await {
            eprintln!("Task execution failed: {}", e);
        }
    });
    ui.run().await?;
    Ok(())

    // match run_handle.join() {
    //     Ok(task_result) => task_result,
    //     Err(panic_box) => {
    //         let panic_info = if let Some(msg) = panic_box.downcast_ref::<&str>() {
    //             msg.to_string()
    //         } else if let Some(msg) = panic_box.downcast_ref::<String>() {
    //             msg.clone()
    //         } else {
    //             format!(
    //                 "Unknown panic; unable to resolve exception type: {:?}",
    //                 panic_box.type_id()
    //             )
    //         };
    //         Err(anyhow!(
    //             "A panic occurred in the task thread: {}",
    //             panic_info
    //         ))?
    //     }
    // }
}
