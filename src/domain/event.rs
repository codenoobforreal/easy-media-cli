use crate::{domain::TaskMetadata, infra::Progress};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    TaskQueueStart {
        total: usize,
    },

    TaskStarted {
        metadata: TaskMetadata,
    },

    TaskProgress {
        id: usize,
        progress: Progress,
    },

    TaskCompleted {
        id: usize,
    },

    /// 任务完成并产出结果
    /// - `String` 相较结构化数据具有更强的通用性。如果后续需要结构化数据：可以给 `Event::TaskResult` 加 `Box<dyn Any>` 字段，下游自己向下转型
    TaskResult {
        id: usize,
        summary: String,
    },

    TaskFailed {
        id: usize,
        error: String,
    },

    TaskCancelled {
        id: usize,
    },

    AllTasksCompleted {
        total: usize,
        success: usize,
        failed: usize,
        cancelled: usize,
    },

    Shutdown,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{domain::test_utils::sample_test_metadata, infra::test_utils::sample_progress};
    use insta::assert_debug_snapshot;

    #[test]
    fn all_variants_can_be_constructed() {
        let _ = Event::TaskQueueStart { total: 10 };
        let _ = Event::TaskStarted {
            metadata: sample_test_metadata(1),
        };
        let _ = Event::TaskProgress {
            id: 1,
            progress: sample_progress(),
        };
        let _ = Event::TaskCompleted { id: 1 };
        let _ = Event::TaskResult {
            id: 1,
            summary: "transcode done".into(),
        };
        let _ = Event::TaskFailed {
            id: 1,
            error: "io error".into(),
        };
        let _ = Event::TaskCancelled { id: 1 };
        let _ = Event::AllTasksCompleted {
            total: 10,
            success: 8,
            failed: 1,
            cancelled: 1,
        };
        let _ = Event::Shutdown;
    }

    #[test]
    fn implements_clone_correctly() {
        let event = Event::TaskCompleted { id: 42 };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[test]
    fn partial_eq_distinguishes_different_events() {
        let e1 = Event::TaskFailed {
            id: 1,
            error: "err".into(),
        };
        let e2 = Event::TaskFailed {
            id: 1,
            error: "err".into(),
        };
        let e3 = Event::TaskFailed {
            id: 2,
            error: "err".into(),
        };
        assert_eq!(e1, e2);
        assert_ne!(e1, e3);
    }

    #[test]
    fn task_started_carries_complete_metadata() {
        let meta = sample_test_metadata(99);
        let event = Event::TaskStarted {
            metadata: meta.clone(),
        };
        assert_debug_snapshot!(event,@r#"
        TaskStarted {
            metadata: TaskMetadata {
                id: 99,
                name: "sample_task_99",
                status: Pending,
                progress: None,
                error: None,
                result: None,
            },
        }
        "#);
    }

    #[test]
    fn all_tasks_completed_fields_accurate() {
        let event = Event::AllTasksCompleted {
            total: 100,
            success: 80,
            failed: 15,
            cancelled: 5,
        };
        assert_debug_snapshot!(event,@"
        AllTasksCompleted {
            total: 100,
            success: 80,
            failed: 15,
            cancelled: 5,
        }
        ");
    }
}
