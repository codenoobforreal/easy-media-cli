//! 跨线程安全包装（对外主入口）

use crate::{
    domain::event::{Event, EventBus},
    ui::{
        CANCEL_MSG, SUCCESS_MSG,
        renderer::{DefaultRenderer, Renderer},
        state::TaskStateStore,
    },
};
use anyhow::{Context, Result, bail};
use std::{
    io::{stderr, stdout},
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    time::{Duration, Instant},
};

/// 跨线程安全包装后的 `UI`，内置事件订阅绑定逻辑
pub struct SyncUi {
    scheduler: RenderScheduler,
    event_rx: Receiver<Event>,
    task_finish_rx: Receiver<Result<()>>,
}

impl SyncUi {
    /// 订阅事件总线，创建 Event 通道并订阅
    pub fn bind_event_bus(
        renderer: Box<dyn Renderer>,
        bus: &dyn EventBus,
        render_interval: Duration,
        task_finish_rx: Receiver<Result<()>>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel::<Event>();

        let handler = Arc::new(move |event: Event| -> Result<()> {
            event_tx
                .send(event)
                .with_context(|| "Event channel disconnected")?;
            Ok(())
        });
        bus.subscribe(handler)?;

        Ok(Self {
            scheduler: RenderScheduler::new(renderer, render_interval),
            event_rx,
            task_finish_rx,
        })
    }

    /// 阻塞主线程，驱动事件消费和渲染，直到任务线程结束
    pub fn block_on_task_thread_finish_channel(&mut self) -> Result<()> {
        let interval = self.scheduler.render_interval();

        loop {
            self.scheduler.tick_render()?;

            // 消费所有已缓冲的事件
            while let Ok(event) = self.event_rx.try_recv() {
                self.scheduler.push_event(event);
            }

            match self.task_finish_rx.try_recv() {
                Ok(result) => return result,
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    bail!("Task execution thread panicked");
                }
            }

            match self.event_rx.recv_timeout(interval) {
                Ok(event) => {
                    self.scheduler.push_event(event);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("Event channel disconnected");
                }
            }
        }
    }

    pub fn render_final(&mut self, is_cancel: bool) -> Result<()> {
        self.scheduler.render_final(is_cancel)
    }
}

/// UI 渲染调度器：只负责渲染节流、调用渲染后端
struct RenderScheduler {
    renderer: Box<dyn Renderer>,
    state_store: TaskStateStore,
    has_state_update: bool,
    last_render_time: Instant,
    render_interval: Duration,
}

impl RenderScheduler {
    fn new(renderer: Box<dyn Renderer>, render_interval: Duration) -> Self {
        Self {
            state_store: TaskStateStore::new(),
            has_state_update: false,
            last_render_time: Instant::now(),
            render_interval,
            renderer,
        }
    }

    fn render_final(&mut self, is_cancelled: bool) -> Result<()> {
        let stats = self.state_store.get_final_stats();
        let message = if is_cancelled {
            CANCEL_MSG
        } else {
            SUCCESS_MSG
        };
        let tasks = self.state_store.tasks();
        self.renderer.render_final(&stats, tasks, message)
    }

    fn render_interval(&self) -> Duration {
        self.render_interval
    }

    /// 推送事件，更新状态
    fn push_event(&mut self, event: Event) {
        self.state_store.handle_event(event);
        self.has_state_update = true;
    }

    /// 达到间隔且有状态更新进行渲染
    fn tick_render(&mut self) -> Result<bool> {
        let now = Instant::now();

        if !self.has_state_update || now - self.last_render_time < self.render_interval {
            return Ok(false);
        }

        let stats = self.state_store.calculate_stats();
        let tasks = self.state_store.tasks();
        self.renderer.render_running(&stats, tasks)?;
        self.has_state_update = false;
        self.last_render_time = now;

        Ok(true)
    }
}

impl Default for RenderScheduler {
    fn default() -> Self {
        let renderer = Box::new(DefaultRenderer::new(stdout(), stderr()));
        Self::new(renderer, Duration::ZERO)
    }
}
