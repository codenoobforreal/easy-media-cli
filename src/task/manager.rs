use crate::{
    common::join_errors_with_summary,
    domain::{
        cancel_token::CancelToken,
        event::{Event, EventBus},
        task::{Task, TaskError, TaskMetadata},
    },
    infra::DefaultCancelToken,
};
use anyhow::Result;
use std::sync::Arc;

/// 控制任务生命周期，发布生命周期事件，无并发
pub struct TaskManager {
    event_bus: Arc<dyn EventBus>,
    cancel_token: Arc<dyn CancelToken>,
}

impl Clone for TaskManager {
    fn clone(&self) -> Self {
        Self {
            event_bus: self.event_bus.clone(),
            cancel_token: self.cancel_token.clone(),
        }
    }
}

impl TaskManager {
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            event_bus,
            cancel_token: Arc::new(DefaultCancelToken::default()),
        }
    }

    /// 订阅 Shutdown 事件
    pub fn bind_shutdown_listener(&self) -> Result<()> {
        let this = self.clone();
        self.event_bus.subscribe(Arc::new(move |event| {
            if matches!(event, Event::Shutdown) {
                this.shutdown();
            }

            Ok(())
        }))
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub fn run_all(&self, tasks: &[Arc<dyn Task>]) -> Result<()> {
        let total = tasks.len();
        let mut success_count = 0;
        let mut failed_count = 0;
        let mut cancelled_count = 0;
        let mut system_errors = Vec::new();

        self.event_bus.publish(Event::TaskQueueStart { total })?;

        for task in tasks {
            if self.is_cancelled() {
                break;
            }

            let id = task.id();
            let task_name = task.name();
            let task_metadata = TaskMetadata::builder().id(id).name(task_name).build();
            self.event_bus.publish(Event::TaskStarted {
                metadata: task_metadata,
            })?;

            match task.run(&self.event_bus, self.cancel_token.as_ref()) {
                Ok(Some(payload)) => {
                    self.event_bus.publish(Event::TaskCompleted {
                        id,
                        payload: Some(payload),
                    })?;
                    success_count += 1;
                }

                Ok(None) => {
                    self.event_bus
                        .publish(Event::TaskCompleted { id, payload: None })?;
                    success_count += 1;
                }

                Err(TaskError::Cancelled) => {
                    self.event_bus.publish(Event::TaskCancelled { id })?;
                    cancelled_count += 1;
                }

                Err(TaskError::Failed(e)) => {
                    self.event_bus.publish(Event::TaskFailed {
                        id,
                        error: e.to_string(),
                    })?;
                    failed_count += 1;
                    system_errors.push(e);
                }
            }
        }

        // 未取消则发布全部完成事件
        if !self.is_cancelled() {
            self.event_bus.publish(Event::AllTasksCompleted {
                total,
                success: success_count,
                failed: failed_count,
                cancelled: cancelled_count,
            })?;
        }

        if !system_errors.is_empty() {
            let summary = format!("{} system error(s) occurred", system_errors.len());
            return Err(join_errors_with_summary(summary, &system_errors));
        }

        Ok(())
    }

    fn shutdown(&self) {
        self.cancel_token.cancel();
    }
}
