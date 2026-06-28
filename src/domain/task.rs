use crate::domain::{
    cancel_token::CancelToken,
    event::{EventBus, TaskResultPayload},
    progress::Progress,
};
use anyhow::Result;
use std::{fmt, sync::Arc};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("Task cancelled by user")]
    Cancelled,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

pub trait Task: Send + Sync + fmt::Debug {
    fn id(&self) -> usize;
    fn name(&self) -> String;
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

    pub fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{common::approx_eq, domain::event::TaskResultPayload};
    use std::{path::PathBuf, time::Duration};

    #[test]
    fn test_task_metadata_status_transitions() {
        let mut meta = TaskMetadata::builder().id(1).name("test").build();
        assert_eq!(meta.status(), Status::Pending);

        meta.mark_running(Some(Progress::new(10.0, Duration::ZERO, None)));
        assert_eq!(meta.status(), Status::Running);
        assert!(approx_eq(
            f64::from(meta.progress().unwrap().percentage()),
            10.0,
            0.1
        ));

        meta.mark_completed(None);
        assert_eq!(meta.status(), Status::Completed);
        assert!(meta.result().is_none());
        assert!(meta.error().is_none());

        let mut meta = TaskMetadata::builder()
            .id(1)
            .name("test")
            .status(Status::Running)
            .build();
        let result = TaskResultPayload::VideoEncoder {
            output_path: PathBuf::from("out.mp4"),
            size_bytes: 1000,
            size_change: 1.0,
        };
        meta.mark_completed(Some(result));
        assert!(meta.result().is_some());
        assert!(meta.error().is_none());

        let mut meta = TaskMetadata::builder().id(2).name("test2").build();
        meta.mark_failed("error".to_string());
        assert_eq!(meta.status(), Status::Failed);
        assert_eq!(meta.error(), Some("error"));
        assert_eq!(meta.result(), None);

        let mut meta = TaskMetadata::builder().id(3).name("test3").build();
        meta.mark_cancelled();
        assert_eq!(meta.status(), Status::Cancelled);
        assert_eq!(meta.error(), None);
        assert_eq!(meta.result(), None);
    }
}
