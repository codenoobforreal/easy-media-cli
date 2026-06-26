use crate::{
    common::join_errors_with_summary,
    domain::{CancelToken, Event, Task, TaskError, TaskMetadata},
    infra::{DefaultCancelToken, EventBus},
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

    /// 订阅事件总线的 Shutdown 信号，收到后触发取消
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
                Ok(()) => {
                    self.event_bus.publish(Event::TaskCompleted { id })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::test_utils::MockTask,
        infra::test_utils::{MockCancelToken, MockEventBus},
    };
    use insta::assert_debug_snapshot;
    use std::assert_matches;

    #[test]
    fn manager_clone_shares_cancel_state() {
        let bus = Arc::new(MockEventBus::default());
        let mgr1 = TaskManager::new(bus.clone());
        let mgr2 = mgr1.clone();
        mgr2.shutdown();
        assert!(mgr1.is_cancelled());
        assert!(mgr2.is_cancelled());
    }

    #[test]
    fn empty_task_queue_emits_correct_events() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        let res = mgr.run_all(&[]);
        assert!(res.is_ok());
        let events = bus.events();
        assert_matches!(events[0], Event::TaskQueueStart { total: 0 });
        assert_matches!(
            events[1],
            Event::AllTasksCompleted {
                total: 0,
                success: 0,
                failed: 0,
                cancelled: 0
            }
        );
    }

    #[test]
    fn single_success_task_full_event_flow() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        let mock_task = MockTask::new(1, "transcode_01");
        let tasks: &[Arc<dyn Task>] = &[Arc::new(mock_task.clone())];
        mgr.run_all(tasks).unwrap();
        // 验证任务确实执行
        assert!(mock_task.was_run());
        let events = bus.events();
        // 事件顺序：队列开始 → 任务启动 → 任务完成 → 全部完成
        assert_eq!(events.len(), 4);
        assert_matches!(events[0], Event::TaskQueueStart { total: 1 });
        assert_matches!(&events[1], Event::TaskStarted{metadata} if metadata.id() == 1);
        assert_matches!(events[2], Event::TaskCompleted { id: 1 });
        assert_matches!(
            events[3],
            Event::AllTasksCompleted {
                success: 1,
                failed: 0,
                cancelled: 0,
                total: 1
            }
        );
    }

    #[test]
    fn single_failed_task_emit_failed_event_and_aggregate_error() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        let mock_task = MockTask::new(2, "corrupt_file");
        mock_task.set_fail("read input stream failed");
        let tasks: &[Arc<dyn Task>] = &[Arc::new(mock_task.clone())];
        let err = mgr.run_all(tasks).unwrap_err();
        assert_debug_snapshot!(err,@r#""1 system error(s) occurred\n- \"read input stream failed\"""#);
        let events = bus.events();
        assert!(events.iter().any(|e| matches!(e, Event::TaskFailed{id:2, error} if error.contains("read input stream failed"))));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::TaskCompleted { .. }))
        );
    }

    #[test]
    fn mixed_success_failed_correct_statistics() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        let task_ok1 = MockTask::new(1, "thumb1");
        let task_err = MockTask::new(2, "meta_parse");
        task_err.set_fail("ffprobe json parse error");
        let task_ok2 = MockTask::new(3, "thumb2");
        let tasks: Vec<Arc<dyn Task>> =
            vec![Arc::new(task_ok1), Arc::new(task_err), Arc::new(task_ok2)];
        let _err = mgr.run_all(&tasks);
        let final_event = bus.events().last().unwrap().clone();
        match final_event {
            Event::AllTasksCompleted {
                total,
                success,
                failed,
                cancelled,
            } => {
                assert_eq!(total, 3);
                assert_eq!(success, 2);
                assert_eq!(failed, 1);
                assert_eq!(cancelled, 0);
            }
            _ => panic!("Final event must be AllTasksCompleted"),
        }
    }

    #[test]
    fn pre_cancel_skip_all_tasks() {
        let bus = Arc::new(MockEventBus::default());
        let cancel = Arc::new(MockCancelToken::default());
        cancel.set_cancelled(true);
        let mgr = TaskManager {
            event_bus: bus.clone(),
            cancel_token: cancel,
        };
        let mock_task = MockTask::new(99, "never_run");
        let tasks: &[Arc<dyn Task>] = &[Arc::new(mock_task.clone())];
        mgr.run_all(tasks).unwrap();
        // 任务完全未执行
        assert!(!mock_task.was_run());
        let events = bus.events();
        // 仅发布队列启动，无任务启动/完成事件，无AllTasksCompleted
        assert_eq!(events.len(), 1);
        assert_matches!(events[0], Event::TaskQueueStart { total: 1 });
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::AllTasksCompleted { .. }))
        );
    }

    #[allow(clippy::similar_names)]
    #[test]
    fn mid_run_cancel_stop_following_tasks() {
        let bus = Arc::new(MockEventBus::default());
        let cancel = Arc::new(MockCancelToken::default());
        let mgr = TaskManager {
            event_bus: bus.clone(),
            cancel_token: cancel.clone(),
        };
        let task1 = MockTask::new(1, "first_task");
        // 第一个任务执行时触发全局取消
        let cancel_clone = cancel.clone();
        task1.set_on_run(move || cancel_clone.set_cancelled(true));
        let task2 = MockTask::new(2, "second_task");
        let tasks: Vec<Arc<dyn Task>> = vec![Arc::new(task1.clone()), Arc::new(task2.clone())];
        mgr.run_all(&tasks).unwrap();
        assert!(task1.was_run());
        assert!(!task2.was_run()); // 第二个任务被跳过
        let started_count = bus
            .events()
            .iter()
            .filter(|e| matches!(e, Event::TaskStarted { .. }))
            .count();
        assert_eq!(started_count, 1);
    }

    #[test]
    fn bind_shutdown_listener_cancel_manager() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        mgr.bind_shutdown_listener().unwrap();
        assert!(!mgr.is_cancelled());
        bus.publish(Event::Shutdown).unwrap();
        assert!(mgr.is_cancelled());
    }

    #[test]
    fn task_returning_cancelled_publishes_event_and_continues() {
        let bus = Arc::new(MockEventBus::default());
        let mgr = TaskManager::new(bus.clone());
        let task_cancel = MockTask::new(1, "voluntary_cancel");
        task_cancel.set_cancelled();
        let task_ok = MockTask::new(2, "still_runs");
        let tasks: Vec<Arc<dyn Task>> = vec![Arc::new(task_cancel), Arc::new(task_ok)];
        mgr.run_all(&tasks).unwrap();
        let events = bus.events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TaskCancelled { id: 1 }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TaskCompleted { id: 2 }))
        );
        let final_event = events.last().expect("should have final event");
        match final_event {
            Event::AllTasksCompleted {
                total,
                success,
                failed,
                cancelled,
            } => {
                assert_eq!(*total, 2);
                assert_eq!(*success, 1);
                assert_eq!(*failed, 0);
                assert_eq!(*cancelled, 1, "cancelled count must be 1");
            }
            other => panic!("Expected AllTasksCompleted, got {other:?}"),
        }
    }
}
