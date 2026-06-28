//! UI 统计数据结构

use crate::domain::{
    event::Event,
    task::{Status, TaskMetadata},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    expected_total: usize,
    total: usize,
    pending: usize,
    running: usize,
    completed: usize,
    failed: usize,
    canceled: usize,
}

impl Stats {
    pub fn new(
        total: usize,
        pending: usize,
        running: usize,
        completed: usize,
        failed: usize,
        canceled: usize,
    ) -> Self {
        Self {
            expected_total: 0,
            total,
            pending,
            running,
            completed,
            failed,
            canceled,
        }
    }

    pub fn with_expected(
        expected_total: usize,
        total: usize,
        pending: usize,
        running: usize,
        completed: usize,
        failed: usize,
        canceled: usize,
    ) -> Self {
        Self {
            expected_total,
            total,
            pending,
            running,
            completed,
            failed,
            canceled,
        }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn running(&self) -> usize {
        self.running
    }

    pub fn completed(&self) -> usize {
        self.completed
    }

    pub fn failed(&self) -> usize {
        self.failed
    }

    pub fn canceled(&self) -> usize {
        self.canceled
    }

    pub fn expected_total(&self) -> usize {
        self.expected_total
    }
}

/// 任务状态存储器：统一管理所有任务的元数据、状态更新、统计计算
#[derive(Debug, Default, Clone)]
pub struct TaskStateStore {
    tasks: Vec<Option<TaskMetadata>>,
    /// 所有任务个数
    expected_total: usize,
    /// 与任务执行状态同步的状态
    final_stats: Option<Stats>,
}

impl TaskStateStore {
    pub fn new() -> Self {
        Self {
            tasks: vec![],
            expected_total: 0,
            final_stats: None,
        }
    }

    /// 接收事件，更新内部任务状态
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::TaskQueueStart { total } => {
                self.expected_total = total;
            }

            Event::TaskStarted { metadata } => {
                self.tasks.push(Some(metadata));
            }

            Event::TaskProgress { id, progress } => {
                if let Some(Some(meta)) = self.tasks.get_mut(id - 1) {
                    meta.mark_running(Some(progress));
                }
            }

            Event::TaskCompleted { id, payload } => {
                if let Some(Some(meta)) = self.tasks.get_mut(id - 1) {
                    meta.mark_completed(payload);
                }
            }

            Event::TaskFailed { id, error } => {
                if let Some(Some(meta)) = self.tasks.get_mut(id - 1) {
                    meta.mark_failed(error);
                }
            }

            Event::TaskCancelled { id } => {
                if let Some(Some(meta)) = self.tasks.get_mut(id - 1) {
                    meta.mark_cancelled();
                }
            }

            Event::AllTasksCompleted {
                total,
                success,
                failed,
                cancelled,
            } => {
                self.final_stats = Some(Stats::with_expected(
                    total, total, 0, 0, success, failed, cancelled,
                ));
            }

            Event::Shutdown => {}
        }
    }

    /// 计算实时全局统计数据
    pub fn calculate_stats(&self) -> Stats {
        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut cancelled = 0;

        for task in &self.tasks {
            match task {
                Some(meta) => match meta.status() {
                    Status::Pending => pending += 1,
                    Status::Running => running += 1,
                    Status::Completed => completed += 1,
                    Status::Failed => failed += 1,
                    Status::Cancelled => cancelled += 1,
                },
                None => pending += 1,
            }
        }

        let total = self.tasks.len();
        Stats::with_expected(
            self.expected_total,
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
            self.calculate_stats()
        }
    }

    pub fn tasks(&self) -> &[Option<TaskMetadata>] {
        &self.tasks
    }
}
