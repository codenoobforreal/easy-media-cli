use crate::{
    ffmpeg_progress::Progress,
    infra::{CancelToken, EventBus},
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
    fn name(&self) -> Option<&str>;
    fn run(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<(), TaskError>;
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskMetadata {
    id: usize,
    name: String,
    status: Status,
    progress: Option<Progress>,
    error: Option<String>,
    result: Option<String>,
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

    pub fn result(&self) -> Option<String> {
        self.result.clone()
    }

    // pub fn set_id(&mut self, id: u64) {
    //     self.id = id;
    // }

    // pub fn set_name(&mut self, name: impl Into<String>) {
    //     self.name = name.into();
    // }

    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    pub fn set_progress(&mut self, progress: Option<Progress>) {
        self.progress = progress;
    }

    pub fn set_error(&mut self, err: Option<impl Into<String>>) {
        self.error = err.map(Into::into);
    }

    pub fn set_result(&mut self, result: Option<impl Into<String>>) {
        self.result = result.map(Into::into);
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct MetadataBuilder {
    id: usize,
    name: String,
    status: Status,
    progress: Option<Progress>,
    error: Option<String>,
    result: Option<String>,
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        ffmpeg_progress::sample_progress,
        infra::{MockCancelToken, MockEventBus},
    };
    use anyhow::anyhow;
    use insta::assert_debug_snapshot;
    use std::{assert_matches, fmt, sync::Mutex};

    pub struct MockTask {
        id: usize,
        name: Option<String>,
        run_result: Arc<Mutex<Option<Result<(), TaskError>>>>,
        run_called: Arc<Mutex<bool>>,
        #[allow(clippy::type_complexity)]
        on_run: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    }

    impl Default for MockTask {
        fn default() -> Self {
            Self {
                id: 0,
                name: None,
                run_result: Arc::new(Mutex::new(Some(Ok(())))),
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
        pub fn new(id: usize, name: Option<&str>) -> Self {
            Self {
                id,
                name: name.map(str::to_string),
                run_result: Arc::new(Mutex::new(Some(Ok(())))),
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
            *self.run_result.lock().unwrap() = Some(Ok(()));
            *self.on_run.lock().unwrap() = None;
        }
    }

    impl Task for MockTask {
        fn id(&self) -> usize {
            self.id
        }

        fn name(&self) -> Option<&str> {
            self.name.as_deref()
        }

        fn run(
            &self,
            _event_bus: &Arc<dyn EventBus>,
            _cancel_token: &dyn CancelToken,
        ) -> Result<(), TaskError> {
            *self.run_called.lock().unwrap() = true;

            let on_run = self.on_run.lock().unwrap();
            if let Some(callback) = &*on_run {
                callback();
            }

            let mut res_guard = self.run_result.lock().unwrap();
            res_guard.take().unwrap_or(Ok(()))
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

    pub fn sample_test_metadata_with_all(id: usize, name: &str, status: Status) -> TaskMetadata {
        let mut meta = TaskMetadata::builder().id(id).name(name).build();
        meta.set_status(status);
        meta
    }

    mod status {
        use super::*;

        #[test]
        fn default_is_pending() {
            assert_eq!(Status::default(), Status::Pending);
        }

        #[test]
        fn partial_eq_works_for_all_variants() {
            assert_eq!(Status::Pending, Status::Pending);
            assert_eq!(Status::Running, Status::Running);
            assert_eq!(Status::Completed, Status::Completed);
            assert_eq!(Status::Failed, Status::Failed);
            assert_eq!(Status::Cancelled, Status::Cancelled);
            assert_ne!(Status::Running, Status::Failed);
        }

        #[test]
        fn set_status_updates_field() {
            let mut meta = TaskMetadata::default();
            assert_eq!(meta.status(), Status::Pending);
            meta.set_status(Status::Running);
            assert_eq!(meta.status(), Status::Running);
            meta.set_status(Status::Cancelled);
            assert_eq!(meta.status(), Status::Cancelled);
        }
    }

    mod builder {
        use super::*;

        #[test]
        fn new_returns_default_values() {
            let meta = MetadataBuilder::new().build();
            assert_debug_snapshot!(meta,@r#"
            TaskMetadata {
                id: 0,
                name: "",
                status: Pending,
                progress: None,
                error: None,
                result: None,
            }
            "#);
        }

        #[test]
        fn chain_call_sets_fields_correctly() {
            let meta = MetadataBuilder::new()
                .id(42)
                .name("transcode_1080p")
                .build();
            assert_debug_snapshot!(meta,@r#"
            TaskMetadata {
                id: 42,
                name: "transcode_1080p",
                status: Pending,
                progress: None,
                error: None,
                result: None,
            }
            "#);
        }

        #[test]
        fn builder_default_matches_struct_default() {
            let from_struct = TaskMetadata::default();
            let from_builder = MetadataBuilder::new().build();
            assert_eq!(from_struct, from_builder);
        }
    }

    mod metadata {
        use super::*;

        #[test]
        fn all_getters_return_expected_values() {
            let meta = TaskMetadata::builder().id(123).name("demo_task").build();
            assert_eq!(meta.id(), 123);
            assert_eq!(meta.name(), "demo_task");
            assert_eq!(meta.status(), Status::Pending);
        }

        #[test]
        fn set_progress_supports_some_and_none() {
            let mut meta = TaskMetadata::default();
            assert!(meta.progress().is_none());
            let prog = sample_progress();
            meta.set_progress(Some(prog));
            assert_eq!(meta.progress().unwrap(), prog);
            meta.set_progress(None);
            assert!(meta.progress().is_none());
        }

        #[test]
        fn set_error_handles_option_correctly() {
            let mut meta = TaskMetadata::default();
            assert!(meta.error().is_none());
            meta.set_error(Some("file not found"));
            assert_debug_snapshot!(meta.error(),@r#"
            Some(
                "file not found",
            )
            "#);
            meta.set_error(None::<String>);
            assert!(meta.error().is_none());
        }

        #[test]
        fn set_result_handles_option_correctly() {
            let mut meta = TaskMetadata::default();
            assert!(meta.result().is_none());
            meta.set_result(Some("output.mp4"));
            assert_debug_snapshot!(meta.result(),@r#"
            Some(
                "output.mp4",
            )
            "#);
            meta.set_result(None::<String>);
            assert!(meta.result().is_none());
        }

        #[test]
        fn clone_preserves_full_state() {
            let original = TaskMetadata::builder().id(7).name("clone_test").build();
            let cloned = original.clone();
            assert_eq!(original, cloned);
        }
    }

    mod task_trait {
        use super::*;

        #[test]
        fn trait_object_works_normally() {
            let task: Arc<dyn Task> = Arc::new(MockTask::new(10, Some("mock_transcode")));
            assert_eq!(task.id(), 10);
            assert_eq!(task.name(), Some("mock_transcode"));
        }

        #[test]
        fn run_invokes_task_logic() {
            let task = MockTask::default();
            let event_bus: Arc<dyn EventBus> = Arc::new(MockEventBus::default());
            let cancel_token = MockCancelToken::default();
            assert!(!*task.run_called.lock().unwrap());
            task.run(&event_bus, &cancel_token).unwrap();
            assert!(*task.run_called.lock().unwrap());
        }

        #[test]
        fn run_returns_cancelled_when_set() {
            let task = MockTask::new(1, Some("cancel_me"));
            task.set_cancelled();
            let bus: Arc<dyn EventBus> = Arc::new(MockEventBus::default());
            let token = MockCancelToken::default();
            let result = task.run(&bus, &token);
            assert_matches!(result, Err(TaskError::Cancelled));
        }
    }
}
