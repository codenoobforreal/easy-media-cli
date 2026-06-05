use crate::{
    error::AppResult,
    event::{Event, EventBus},
    task::{Info, SharedTask, Status},
};
use futures::future::join_all;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    spawn,
    sync::{Mutex, Semaphore},
};

#[derive(Clone)]
pub struct Executor {
    event_bus: EventBus,
    semaphore: Arc<Semaphore>,
    tasks: Arc<Mutex<HashMap<u64, Arc<Mutex<Info>>>>>,
}

impl Executor {
    pub fn new(concurrency: usize, event_bus: EventBus) -> Self {
        Self {
            event_bus: event_bus.clone(),
            semaphore: Arc::new(Semaphore::new(concurrency)),
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_tasks(&self, tasks: &[SharedTask]) -> AppResult<()> {
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
        Ok(())
    }

    pub async fn start_event_listener(&self) {
        let mut receiver = self.event_bus.subscribe();
        let tasks = self.tasks.clone();

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
                        _ => {}
                    }
                }
            }
        });
    }

    pub async fn run_all(&self, tasks: Vec<SharedTask>) -> AppResult<()> {
        self.add_tasks(&tasks).await?;

        let futures = tasks.into_iter().map(|task| {
            let semaphore = self.semaphore.clone();
            let event_bus = self.event_bus.clone();

            async move {
                let _permit = semaphore.acquire().await.unwrap();

                let id = task.id();
                event_bus.publish(Event::TaskStarted { id }).ok();

                match task.run(event_bus.clone()).await {
                    Ok(_) => {
                        event_bus.publish(Event::TaskCompleted { id }).ok();
                    }
                    Err(e) => {
                        event_bus
                            .publish(Event::TaskFailed {
                                id,
                                error: e.to_string(),
                            })
                            .ok();
                    }
                }
            }
        });

        join_all(futures).await;

        self.event_bus.publish(Event::AllTasksCompleted)?;

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
