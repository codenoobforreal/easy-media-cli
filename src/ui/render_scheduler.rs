//! 渲染调度核心

use crate::{
    domain::Event,
    ui::{CANCEL_MSG, DefaultRenderer as TerminalRenderer, Renderer, SUCCESS_MSG, TaskStateStore},
};
use anyhow::Result;
use std::{
    io::{stderr, stdout},
    time::{Duration, Instant},
};

/// UI 渲染调度器：只负责渲染节流、调用渲染后端
pub struct RenderScheduler {
    renderer: Box<dyn Renderer>,
    state_store: TaskStateStore,
    has_state_update: bool,
    last_render_time: Instant,
    render_interval: Duration,
}

impl RenderScheduler {
    /// 默认使用 `TerminalRenderer` 的 `new` 方法
    pub fn new(render_interval: Duration) -> Self {
        let renderer = Box::new(TerminalRenderer::new(stdout(), stderr()));
        Self::with_renderer(renderer, render_interval)
    }

    /// 推送单个事件，更新本地状态（由同步事件总线回调调用）
    pub fn push_event(&mut self, event: &Event) {
        self.state_store.handle_event(event);
        self.has_state_update = true;
    }

    /// 达到间隔且有状态更新时才刷新终端
    pub fn tick_render(&mut self) -> Result<bool> {
        let now = Instant::now();
        if now - self.last_render_time < self.render_interval {
            return Ok(false);
        }

        if !self.has_state_update {
            return Ok(false);
        }

        let stats = self.state_store.calculate_overall_stats();
        let task_list = self.state_store.task_list();
        self.renderer.render_running(&stats, &task_list)?;
        self.has_state_update = false;
        self.last_render_time = now;

        Ok(true)
    }

    pub fn render_final(&mut self, is_cancelled: bool) -> Result<()> {
        let stats = self.state_store.get_final_stats();

        let message = if is_cancelled {
            CANCEL_MSG
        } else {
            SUCCESS_MSG
        };
        let task_list = self.state_store.task_list();
        self.renderer.render_final(&stats, &task_list, message)
    }

    pub fn with_renderer(renderer: Box<dyn Renderer>, render_interval: Duration) -> Self {
        Self {
            renderer,
            state_store: TaskStateStore::new(),
            has_state_update: false,
            last_render_time: Instant::now(),
            render_interval,
        }
    }

    pub fn render_interval(&self) -> Duration {
        self.render_interval
    }

    #[cfg(test)]
    pub fn state_store(&self) -> TaskStateStore {
        self.state_store.clone()
    }

    // 将上次渲染时间回溯到 200ms 前，确保满足 100ms 间隔要求
    #[cfg(test)]
    pub(crate) fn skip_render_interval(&mut self) {
        use std::time::Duration;
        self.last_render_time = Instant::now()
            .checked_sub(Duration::from_millis(200))
            .unwrap();
    }
}

impl Default for RenderScheduler {
    fn default() -> Self {
        Self::new(Duration::ZERO)
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::{cli::GlobalConfig, ui::test_utils::MockRenderer};
    use std::sync::{Arc, Mutex};

    /// 构造调度器并返回调用计数句柄
    pub fn sample_ui_scheduler() -> (RenderScheduler, Arc<Mutex<usize>>, Arc<Mutex<usize>>) {
        let mock = MockRenderer::default();
        let running = mock.running_calls.clone();
        let final_ = mock.final_calls.clone();
        let config = GlobalConfig::parser_default();
        let sched = RenderScheduler::with_renderer(
            Box::new(mock),
            Duration::from_millis(config.render_interval_ms),
        );
        (sched, running, final_)
    }
}

#[cfg(test)]
pub mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{domain::TaskMetadata, ui::test_utils::sample_ui_scheduler};

    #[test]
    fn default_has_no_pending_updates() {
        let (s, _, _) = sample_ui_scheduler();
        assert!(!s.has_state_update);
        assert!(s.state_store.task_list().is_empty());
    }

    #[test]
    fn push_event_marks_dirty_and_updates_state() {
        let (mut s, _, _) = sample_ui_scheduler();
        s.push_event(&Event::TaskStarted {
            metadata: TaskMetadata::builder().id(1).name("task1").build(),
        });
        assert!(s.has_state_update);
        assert_eq!(s.state_store.calculate_overall_stats().total(), 1);
    }

    #[test]
    fn tick_no_update_skips_render() {
        let (mut s, running, _) = sample_ui_scheduler();
        s.last_render_time = Instant::now().checked_sub(Duration::from_secs(1)).unwrap(); // 时间满足
        let rendered = s.tick_render().unwrap();
        assert!(!rendered);
        assert_eq!(*running.lock().unwrap(), 0);
    }

    #[test]
    fn tick_within_interval_skips_render_even_with_update() {
        let (mut s, running, _) = sample_ui_scheduler();
        s.push_event(&Event::TaskQueueStart { total: 10 });
        let rendered = s.tick_render().unwrap();
        assert!(!rendered);
        assert_eq!(*running.lock().unwrap(), 0);
        assert!(s.has_state_update); // 标记保留，等时间到再渲染
    }

    #[test]
    fn tick_with_update_and_interval_passed_renders_once() {
        let (mut s, running, _) = sample_ui_scheduler();
        s.push_event(&Event::TaskQueueStart { total: 10 });
        s.last_render_time = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        let rendered = s.tick_render().unwrap();
        assert!(rendered);
        assert_eq!(*running.lock().unwrap(), 1);
        assert!(!s.has_state_update); // 渲染后清除脏标记
    }

    #[test]
    fn render_final_success_uses_success_message() {
        let (mut s, _, final_) = sample_ui_scheduler();
        s.render_final(false).unwrap();
        assert_eq!(*final_.lock().unwrap(), 1);
    }

    #[test]
    fn render_final_cancel_uses_cancel_message() {
        let (mut s, _, final_) = sample_ui_scheduler();
        s.render_final(true).unwrap();
        assert_eq!(*final_.lock().unwrap(), 1);
    }
}
