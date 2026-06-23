//! 任务状态存储器

use crate::{
    domain::{Event, Status, TaskMetadata},
    ui::Stats,
};
use std::collections::HashMap;

/// 任务状态存储器：统一管理所有任务的元数据、状态更新、统计计算
#[derive(Debug, Default, Clone)]
pub struct TaskStateStore {
    tasks: HashMap<usize, TaskMetadata>,
    expected_total: Option<usize>,
    final_stats: Option<Stats>,
}

impl TaskStateStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            expected_total: None,
            final_stats: None,
        }
    }

    /// 接收事件，更新内部任务状态
    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::TaskQueueStart { total } => {
                self.expected_total = Some(*total);
            }

            Event::TaskStarted { metadata } => {
                self.tasks.insert(metadata.id(), metadata.clone());
            }

            Event::TaskProgress { id, progress } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.mark_running(Some(*progress));
                }
            }

            Event::TaskCompleted { id } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.mark_completed(None);
                }
            }

            Event::TaskResult { id, summary } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.set_result(Some(summary.clone()));
                }
            }

            Event::TaskFailed { id, error } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.mark_failed(error.clone());
                }
            }

            Event::TaskCancelled { id } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.mark_cancelled();
                }
            }

            Event::AllTasksCompleted {
                total,
                success,
                failed,
                cancelled,
            } => {
                self.final_stats = Some(Stats::with_expected(
                    *total, *total, 0, 0, *success, *failed, *cancelled,
                ));
            }

            Event::Shutdown => {}
        }
    }

    /// 计算实时全局统计数据
    pub fn calculate_overall_stats(&self) -> Stats {
        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut cancelled = 0;

        for task in self.tasks.values() {
            match task.status() {
                Status::Pending => pending += 1,
                Status::Running => running += 1,
                Status::Completed => completed += 1,
                Status::Failed => failed += 1,
                Status::Cancelled => cancelled += 1,
            }
        }

        let total = self.tasks.len();

        Stats::with_expected(
            self.expected_total.unwrap_or(0),
            total,
            pending,
            running,
            completed,
            failed,
            cancelled,
        )
    }

    /// 获取最终统计数据，优先使用外部聚合数据
    pub fn get_final_stats(&self) -> Stats {
        if let Some(stats) = self.final_stats {
            stats
        } else {
            let total = self.tasks.len();
            let completed = self
                .tasks
                .values()
                .filter(|t| t.status() == Status::Completed)
                .count();
            let failed = self
                .tasks
                .values()
                .filter(|t| t.status() == Status::Failed)
                .count();
            let cancelled = self
                .tasks
                .values()
                .filter(|t| t.status() == Status::Cancelled)
                .count();
            let pending = total - completed - failed - cancelled;

            Stats::new(total, pending, 0, completed, failed, cancelled)
        }
    }

    pub fn task_list(&self) -> Vec<(usize, &TaskMetadata)> {
        let mut entries: Vec<_> = self.tasks.iter().map(|(&id, meta)| (id, meta)).collect();
        entries.sort_by_key(|(id, _)| *id); // 决定顺序：按任务 ID 升序
        entries
    }

    /// 清空所有状态，支持复用
    pub fn clear(&mut self) {
        self.tasks.clear();
        self.final_stats = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::test_utils::sample_test_metadata_with_id_name, infra::test_utils::sample_progress,
    };
    use insta::assert_debug_snapshot;

    #[test]
    fn new_store_is_empty() {
        let store = TaskStateStore::new();
        assert!(store.task_list().is_empty());
        assert_eq!(store.calculate_overall_stats().total(), 0);
    }

    #[test]
    fn task_queue_start_sets_expected_total_but_not_total() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskQueueStart { total: 10 });
        let stats = store.calculate_overall_stats();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.expected_total(), 10);
    }

    #[test]
    fn total_reflects_actual_registered_tasks() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "a"),
        });
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(2, "b"),
        });
        let stats = store.calculate_overall_stats();
        assert_eq!(stats.total(), 2);
    }

    #[test]
    fn task_started_adds_new_task() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "task_001"),
        });
        assert_eq!(store.task_list().len(), 1);
        assert_debug_snapshot!(store.task_list().first(),@r#"
        Some(
            (
                1,
                TaskMetadata {
                    id: 1,
                    name: "task_001",
                    status: Pending,
                    progress: None,
                    error: None,
                    result: None,
                },
            ),
        )
        "#);
    }

    #[test]
    fn task_progress_updates_running_state() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskProgress {
            id: 1,
            progress: sample_progress(),
        });
        assert_debug_snapshot!(store.task_list().first(),@r#"
        Some(
            (
                1,
                TaskMetadata {
                    id: 1,
                    name: "t1",
                    status: Running,
                    progress: Some(
                        Progress {
                            percentage: 0.0,
                            elapsed: 0ns,
                            eta: None,
                        },
                    ),
                    error: None,
                    result: None,
                },
            ),
        )
        "#);
    }

    #[test]
    fn task_progress_ignores_unknown_id() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskProgress {
            id: 999,
            progress: sample_progress(),
        });
        assert!(store.task_list().is_empty());
    }

    #[test]
    fn task_completed_updates_status() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskCompleted { id: 1 });
        assert_eq!(
            store.task_list().first().unwrap().1.status(),
            Status::Completed
        );
    }

    #[test]
    fn task_result_sets_summary() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskCompleted { id: 1 });
        store.handle_event(&Event::TaskResult {
            id: 1,
            summary: "output.mp4".into(),
        });
        assert_eq!(
            store.task_list().first().unwrap().1.result().unwrap(),
            "output.mp4"
        );
    }

    #[test]
    fn task_failed_sets_error_and_status() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskFailed {
            id: 1,
            error: "io error".into(),
        });
        assert_debug_snapshot!(store.task_list().first(),@r#"
        Some(
            (
                1,
                TaskMetadata {
                    id: 1,
                    name: "t1",
                    status: Failed,
                    progress: None,
                    error: Some(
                        "io error",
                    ),
                    result: None,
                },
            ),
        )
        "#);
    }

    #[test]
    fn task_cancelled_updates_status() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskCancelled { id: 1 });
        assert_eq!(
            store.task_list().first().unwrap().1.status(),
            Status::Cancelled
        );
    }

    #[test]
    fn all_tasks_completed_sets_final_stats() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::AllTasksCompleted {
            total: 10,
            success: 8,
            failed: 1,
            cancelled: 1,
        });
        let stats = store.get_final_stats();
        assert_debug_snapshot!(stats,@"
        Stats {
            expected_total: 10,
            total: 10,
            pending: 0,
            running: 0,
            completed: 8,
            failed: 1,
            canceled: 1,
        }
        ");
    }

    #[test]
    fn shutdown_event_does_nothing() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::Shutdown);
        assert!(store.task_list().is_empty());
    }

    #[test]
    fn calculate_overall_stats_sums_all_statuses() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskQueueStart { total: 4 });
        (1..=4).for_each(|i| {
            store.handle_event(&Event::TaskStarted {
                metadata: sample_test_metadata_with_id_name(i, &format!("t{i}")),
            });
        });
        store.handle_event(&Event::TaskProgress {
            id: 1,
            progress: sample_progress(),
        });
        store.handle_event(&Event::TaskCompleted { id: 2 });
        store.handle_event(&Event::TaskFailed {
            id: 3,
            error: "err".into(),
        });
        let stats = store.calculate_overall_stats();
        assert_debug_snapshot!(stats,@"
        Stats {
            expected_total: 4,
            total: 4,
            pending: 1,
            running: 1,
            completed: 1,
            failed: 1,
            canceled: 0,
        }
        ");
    }

    #[test]
    fn get_final_stats_prefers_external_aggregation() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::AllTasksCompleted {
            total: 100,
            success: 90,
            failed: 5,
            cancelled: 5,
        });
        assert_eq!(store.get_final_stats().total(), 100);
    }

    #[test]
    fn get_final_stats_falls_back_to_internal_calculation() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::TaskCompleted { id: 1 });
        let stats = store.get_final_stats();
        assert_debug_snapshot!(stats,@"
        Stats {
            expected_total: 0,
            total: 1,
            pending: 0,
            running: 0,
            completed: 1,
            failed: 0,
            canceled: 0,
        }
        ");
    }

    #[test]
    fn clear_resets_all_state() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskQueueStart { total: 10 });
        store.handle_event(&Event::TaskStarted {
            metadata: sample_test_metadata_with_id_name(1, "t1"),
        });
        store.handle_event(&Event::AllTasksCompleted {
            total: 10,
            success: 1,
            failed: 0,
            cancelled: 0,
        });
        store.clear();
        assert!(store.task_list().is_empty());
        assert_eq!(store.calculate_overall_stats().total(), 0);
    }

    #[test]
    fn expected_total_preserved_in_overall_stats() {
        let mut store = TaskStateStore::new();
        store.handle_event(&Event::TaskQueueStart { total: 5 });
        for i in 1..=3 {
            store.handle_event(&Event::TaskStarted {
                metadata: sample_test_metadata_with_id_name(i, "t"),
            });
        }
        let stats = store.calculate_overall_stats();
        assert_eq!(stats.expected_total(), 5);
        assert_eq!(stats.total(), 3);
    }
}
