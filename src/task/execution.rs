use crate::{
    common::media_scan::collect_videos,
    domain::{event::EventBus, task::Task},
    infra::FileSystem,
    task::manager::TaskManager,
    ui::{renderer::Renderer, sync_ui::SyncUi},
};
use anyhow::{Result, bail};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

/// 构建任务列表：收集视频并调用任务工厂生成任务
pub fn build_task_list<F>(
    input: &PathBuf,
    depth: Option<u8>,
    file_system: &dyn FileSystem,
    task_factory: F,
) -> Result<Vec<Arc<dyn Task>>>
where
    F: Fn(usize, PathBuf) -> Result<Arc<dyn Task>>,
{
    let depth = depth.or(Some(0));
    let videos = collect_videos(file_system, input, depth)?;
    if videos.is_empty() {
        bail!("no video found in path: \n{}", input.display());
    }

    let mut tasks = Vec::with_capacity(videos.len());
    for (idx, video) in videos.into_iter().enumerate() {
        let task_id = idx + 1;
        tasks.push(task_factory(task_id, video)?);
    }

    Ok(tasks)
}

/// 执行任务列表，驱动 UI 渲染，直到所有任务完成或被取消
pub fn run_tasks_with_ui(
    tasks: Vec<Arc<dyn Task>>,
    event_bus: Arc<dyn EventBus>,
    renderer: Box<dyn Renderer>,
    render_interval: Duration,
) -> Result<()> {
    let (tasks_finish_tx, tasks_finish_rx) = mpsc::channel::<Result<()>>();

    let mut ui = SyncUi::bind_event_bus(
        renderer,
        event_bus.as_ref(),
        render_interval,
        tasks_finish_rx,
    )?;

    let task_manager = TaskManager::new(event_bus);
    task_manager.bind_shutdown_listener()?;

    let task_manager_clone = task_manager.clone();
    thread::spawn(move || {
        let res = task_manager_clone.run_all(&tasks);
        let _ = tasks_finish_tx.send(res);
    });

    ui.block_on_task_thread_finish_channel()?;
    ui.render_final(task_manager.is_cancelled())?;

    Ok(())
}
