use crate::{
    event::{Event, EventBus},
    task::{Info, SharedTask, Status},
};
use anyhow::{Context, Error, Result, anyhow};
use futures::future::join_all;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    select, spawn,
    sync::{Mutex, Semaphore},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Executor {
    event_bus: EventBus,
    semaphore: Arc<Semaphore>,
    tasks: Arc<Mutex<HashMap<u64, Arc<Mutex<Info>>>>>,
    cancel_token: CancellationToken,
}

impl Executor {
    pub fn new(concurrency: usize, event_bus: EventBus) -> Self {
        Self {
            event_bus: event_bus.clone(),
            semaphore: Arc::new(Semaphore::new(concurrency)),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn shutdown(&self) {
        self.semaphore.close();
        self.cancel_token.cancel();
    }

    pub async fn add_tasks(&self, tasks: &[SharedTask]) {
        let mut tasks_lock = self.tasks.lock().await;
        for task in tasks {
            let id = task.id();
            let info = Info::builder()
                .id(id)
                .name(task.name())
                .file_path(task.file_path())
                .file_name(task.file_name())
                .build();

            tasks_lock.insert(id, Arc::new(Mutex::new(info)));
        }
    }

    pub async fn start_event_listener(&self) {
        let mut receiver = self.event_bus.subscribe();
        let tasks = self.tasks.clone();
        let executor = self.clone();
        spawn(async move {
            while let Ok(event) = receiver.recv().await {
                let task_arc = {
                    let map_lock = tasks.lock().await;
                    map_lock.get(&event.get_id()).cloned()
                };
                if let Some(task_mutex) = task_arc {
                    let mut task = task_mutex.lock().await;
                    match event {
                        Event::TaskStarted { .. } => {
                            task.set_status(Status::Running);
                        }
                        Event::TaskProgress { progress, .. } => {
                            task.set_progress(progress);
                        }
                        Event::TaskCompleted { .. } => {
                            task.set_status(Status::Completed);
                        }
                        Event::TaskFailed { error, .. } => {
                            task.set_status(Status::Failed);
                            task.set_error(Some(error));
                        }
                        Event::Shutdown => {
                            executor.shutdown();
                            break;
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    pub async fn run_all(&self, tasks: Vec<SharedTask>) -> Result<()> {
        self.add_tasks(&tasks).await;
        let child_token = self.cancel_token.child_token();
        let futures = tasks.into_iter().map(|task| {
            let semaphore = self.semaphore.clone();
            let event_bus = self.event_bus.clone();
            let token = child_token.clone();
            let id = task.id();
            async move {
                select! {
                    result = async {
                        let _permit = semaphore
                            .acquire()
                            .await
                            .with_context(|| format!("Failed to acquire semaphore permit for task"))?;
                        event_bus
                            .publish(Event::TaskStarted { id })?;
                        task.run(event_bus.clone(),token.clone()).await
                    } => {
                        match result {
                            Ok(_) => {
                                event_bus
                                    .publish(Event::TaskCompleted { id })?
                            }
                            Err(e) => {
                                event_bus
                                    .publish(Event::TaskFailed {
                                        id,
                                        error: e.to_string(),
                                    })?
                            }
                        }
                    }
                    _ = token.cancelled() => {
                        event_bus.publish(Event::TaskFailed {
                            id,
                            error: "Task cancelled by user".to_string(),
                        })?
                    }
                }
                Ok::<_,Error>(())
                // run_result.with_context(|| format!("task {} execution failed", id))
            }
        });
        let task_results = join_all(futures).await;
        let errors: Vec<Error> = task_results
            .into_iter()
            .filter_map(|res| res.err())
            .collect();
        if !errors.is_empty() {
            let mut main_error = anyhow!("{} task(s) failed", errors.len());
            for err in errors {
                main_error = main_error.context(err);
            }
            return Err(main_error);
        }
        if !self.cancel_token.is_cancelled() {
            self.event_bus.publish(Event::AllTasksCompleted)?;
        }
        Ok(())
    }

    pub async fn get_task_stats(&self) -> (usize, usize, usize, usize, usize) {
        let task_arcs = {
            let map_lock = self.tasks.lock().await;
            map_lock.values().cloned().collect::<Vec<_>>()
        };

        let total = task_arcs.len();
        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;

        for arc in task_arcs {
            let task = arc.lock().await;
            match task.status() {
                Status::Pending => pending += 1,
                Status::Running => running += 1,
                Status::Completed => completed += 1,
                Status::Failed => failed += 1,
            }
        }

        (total, pending, running, completed, failed)
    }

    pub async fn get_running_tasks(&self) -> Vec<Info> {
        let task_arcs = {
            let map_lock = self.tasks.lock().await;
            map_lock.values().cloned().collect::<Vec<_>>()
        };

        let mut result = Vec::new();
        for arc in task_arcs {
            let task = arc.lock().await;
            if task.status() == Status::Running {
                result.push(task.clone());
            }
        }
        result
    }

    pub async fn get_failed_tasks(&self) -> Vec<Info> {
        let task_arcs = {
            let map_lock = self.tasks.lock().await;
            map_lock.values().cloned().collect::<Vec<_>>()
        };

        let mut result = Vec::new();
        for arc in task_arcs {
            let task = arc.lock().await;
            if task.status() == Status::Failed {
                result.push(task.clone());
            }
        }
        result
    }
}
