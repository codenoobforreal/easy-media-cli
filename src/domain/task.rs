use crate::{
    domain::{CancelToken, TaskResultPayload},
    infra::{EventBus, Progress},
};
use anyhow::Result;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("Task cancelled by user")]
    Cancelled,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

pub trait Task: Send + Sync {
    fn id(&self) -> usize;
    fn name(&self) -> &str;
    fn run(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<Option<TaskResultPayload>, TaskError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct TaskMetadata {
    id: usize,
    name: String,
    status: Status,
    progress: Option<Progress>,
    error: Option<String>,
    result: Option<TaskResultPayload>,
}

impl TaskMetadata {
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::new()
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn progress(&self) -> Option<Progress> {
        self.progress
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn result(&self) -> Option<TaskResultPayload> {
        self.result.clone()
    }

    // pub fn set_id(&mut self, id: u64) {
    //     self.id = id;
    // }

    // pub fn set_name(&mut self, name: impl Into<String>) {
    //     self.name = name.into();
    // }

    /// 标记任务开始执行，更新进度
    pub fn mark_running(&mut self, progress: Option<Progress>) {
        self.status = Status::Running;
        self.progress = progress;
        self.error = None;
        self.result = None;
    }

    /// 标记任务成功完成，可附带结果
    pub fn mark_completed(&mut self, result: Option<TaskResultPayload>) {
        self.status = Status::Completed;
        self.result = result;
        self.error = None;
    }

    /// 标记任务失败，记录错误信息
    pub fn mark_failed(&mut self, error: String) {
        self.status = Status::Failed;
        self.error = Some(error);
        self.result = None;
    }

    /// 标记任务被取消
    pub fn mark_cancelled(&mut self) {
        self.status = Status::Cancelled;
        self.error = None;
        self.result = None;
    }

    pub fn set_result(&mut self, result: Option<TaskResultPayload>) {
        debug_assert!(
            self.status == Status::Completed,
            "set_result should only be called when status is Completed"
        );
        self.result = result;
    }

    pub fn set_error(&mut self, err: Option<impl Into<String>>) {
        self.error = err.map(Into::into);
    }
}

#[derive(Debug, Default, Clone)]
pub struct MetadataBuilder {
    id: usize,
    name: String,
    status: Status,
    progress: Option<Progress>,
    error: Option<String>,
    result: Option<TaskResultPayload>,
}

impl MetadataBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: usize) -> Self {
        self.id = id;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    // pub fn status(mut self, status: Status) -> Self {
    //     self.status = status;
    //     self
    // }

    // pub fn progress(mut self, progress: Option<Progress>) -> Self {
    //     self.progress = progress;
    //     self
    // }

    // pub fn error(mut self, err: Option<impl Into<String>>) -> Self {
    //     self.error = err.map(Into::into);
    //     self
    // }

    pub fn build(self) -> TaskMetadata {
        TaskMetadata {
            id: self.id,
            name: self.name,
            status: self.status,
            progress: self.progress,
            error: self.error,
            result: self.result,
        }
    }
}

#[allow(clippy::type_complexity)]
#[cfg(test)]
pub mod test_utils {
    use super::*;
    use anyhow::anyhow;
    use std::{fmt, sync::Mutex};

    pub struct MockTask {
        id: usize,
        name: String,
        run_result: Arc<Mutex<Option<Result<Option<TaskResultPayload>, TaskError>>>>,
        pub run_called: Arc<Mutex<bool>>,
        #[allow(clippy::type_complexity)]
        on_run: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    }

    impl Default for MockTask {
        fn default() -> Self {
            Self {
                id: 0,
                name: String::new(),
                run_result: Arc::new(Mutex::new(Some(Ok(None)))),
                run_called: Arc::new(Mutex::new(false)),
                on_run: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl fmt::Debug for MockTask {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("MockTask")
                .field("id", &self.id)
                .field("name", &self.name)
                .field("run_called", &*self.run_called.lock().unwrap())
                .field("run_result", &"<hidden Result<(), anyhow::Error>>")
                .field("on_run", &"<opaque callback>")
                .finish()
        }
    }

    impl Clone for MockTask {
        fn clone(&self) -> Self {
            Self {
                id: self.id,
                name: self.name.clone(),
                run_result: Arc::clone(&self.run_result),
                run_called: Arc::clone(&self.run_called),
                on_run: Arc::clone(&self.on_run),
            }
        }
    }

    impl MockTask {
        pub fn new(id: usize, name: &str) -> Self {
            Self {
                id,
                name: name.into(),
                run_result: Arc::new(Mutex::new(Some(Ok(None)))),
                run_called: Arc::new(Mutex::new(false)),
                on_run: Arc::new(Mutex::new(None)),
            }
        }

        pub fn set_fail(&self, err_msg: &'static str) {
            *self.run_result.lock().unwrap() = Some(Err(TaskError::Failed(anyhow!(err_msg))));
        }

        pub fn set_cancelled(&self) {
            *self.run_result.lock().unwrap() = Some(Err(TaskError::Cancelled));
        }

        pub fn set_on_run<F: Fn() + Send + Sync + 'static>(&self, f: F) {
            *self.on_run.lock().unwrap() = Some(Arc::new(f));
        }

        pub fn was_run(&self) -> bool {
            *self.run_called.lock().unwrap()
        }

        pub fn reset(&self) {
            *self.run_called.lock().unwrap() = false;
            *self.run_result.lock().unwrap() = Some(Ok(None));
            *self.on_run.lock().unwrap() = None;
        }
    }

    impl Task for MockTask {
        fn id(&self) -> usize {
            self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn run(
            &self,
            _event_bus: &Arc<dyn EventBus>,
            _cancel_token: &dyn CancelToken,
        ) -> Result<Option<TaskResultPayload>, TaskError> {
            *self.run_called.lock().unwrap() = true;

            let on_run = self.on_run.lock().unwrap();
            if let Some(callback) = &*on_run {
                callback();
            }

            let mut res_guard = self.run_result.lock().unwrap();
            res_guard.take().unwrap_or(Ok(None))
        }
    }

    pub fn sample_test_metadata(id: usize) -> TaskMetadata {
        TaskMetadata::builder()
            .id(id)
            .name(format!("sample_task_{id}"))
            .build()
    }

    pub fn sample_test_metadata_with_id_name(id: usize, name: &str) -> TaskMetadata {
        TaskMetadata::builder().id(id).name(name).build()
    }
}
