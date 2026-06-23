//! 跨线程安全包装（对外主入口）

use crate::{
    infra::EventBus,
    ui::{RenderScheduler, Renderer},
};
use anyhow::{Result, anyhow};
use std::{
    sync::{
        Arc, Mutex, MutexGuard,
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

/// 跨线程安全包装后的 `UI`，内置事件订阅绑定逻辑
#[derive(Clone)]
pub struct SyncUi(Arc<Mutex<RenderScheduler>>);

impl SyncUi {
    /// 创建并自动订阅事件总线，将所有事件自动转发到 `UI`
    pub fn bind_event_bus(renderer: Box<dyn Renderer>, bus: &dyn EventBus) -> Result<Self> {
        let inner = Arc::new(Mutex::new(RenderScheduler::with_renderer(renderer)));
        let inner_clone = inner.clone();
        bus.subscribe(Arc::new(move |event| {
            let mut guard = inner_clone
                .lock()
                .map_err(|poison| anyhow!("UI mutex poisoned: {poison}"))?;
            guard.push_event(&event);
            Ok(())
        }))?;

        Ok(Self(inner))
    }

    /// 阻塞主线程，驱动节流渲染，等待任务子线程执行完毕
    pub fn block_on_task_thread_finish_channel(&self, rx: &Receiver<Result<()>>) -> Result<()> {
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return result,

                Err(RecvTimeoutError::Timeout) => {
                    let mut ui = self.lock()?;
                    ui.tick_render()?;
                }

                Err(RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!(
                        "Task execution thread panicked or exited unexpectedly"
                    ));
                }
            }
        }
    }

    pub fn render_final(&self, is_cancel: bool) -> Result<()> {
        let mut ui = self.lock()?;
        ui.render_final(is_cancel)
    }

    fn lock(&self) -> Result<MutexGuard<'_, RenderScheduler>> {
        self.0
            .lock()
            .map_err(|poison| anyhow!("UI mutex poisoned: {poison}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{Event, TaskMetadata},
        infra::test_utils::MockEventBus,
        ui::test_utils::MockRenderer,
    };
    use insta::assert_debug_snapshot;
    use std::{sync::mpsc, thread};

    #[test]
    fn bind_event_bus_forwards_events_to_inner() {
        let bus = MockEventBus::default();
        let renderer = Box::new(MockRenderer::default());
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        bus.publish(Event::TaskStarted {
            metadata: TaskMetadata::builder().id(1).name("task1").build(),
        })
        .unwrap();
        let inner = ui.lock().unwrap();
        assert_eq!(inner.state_store().calculate_overall_stats().total(), 1);
    }

    #[test]
    fn clone_shares_same_inner_state() {
        let bus = MockEventBus::default();
        let renderer = Box::new(MockRenderer::default());
        let ui1 = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        let ui2 = ui1.clone();
        bus.publish(Event::TaskStarted {
            metadata: TaskMetadata::builder().id(42).name("task42").build(),
        })
        .unwrap();
        let t1 = ui1
            .lock()
            .unwrap()
            .state_store()
            .calculate_overall_stats()
            .total();
        let t2 = ui2
            .lock()
            .unwrap()
            .state_store()
            .calculate_overall_stats()
            .total();
        assert_eq!(t1, t2);
        assert_eq!(t1, 1); // 只有 1 个任务启动
    }

    #[test]
    fn lock_provides_mutable_access() {
        let bus = MockEventBus::default();
        let renderer = Box::new(MockRenderer::default());
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        {
            let mut guard = ui.lock().unwrap();
            guard.push_event(&Event::TaskStarted {
                metadata: TaskMetadata::builder().id(1).name("task1").build(),
            });
        }
        let guard = ui.lock().unwrap();
        assert_eq!(guard.state_store().calculate_overall_stats().total(), 1);
    }

    #[test]
    fn render_final_delegates_to_inner() {
        let bus = MockEventBus::default();
        let renderer = MockRenderer::default();
        let final_calls = Arc::clone(&renderer.final_calls);
        let last_msg = Arc::clone(&renderer.last_msg);
        let renderer = Box::new(renderer);
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        ui.render_final(false).unwrap();
        assert_eq!(*final_calls.lock().unwrap(), 1);
        assert_debug_snapshot!(*last_msg.lock().unwrap(),@r#"
        Some(
            "All tasks were processed successfully!",
        )
        "#);
    }

    #[test]
    fn tick_render_delegates_to_inner() {
        let bus = MockEventBus::default();
        let renderer = MockRenderer::default();
        let running_calls = Arc::clone(&renderer.running_calls);
        let renderer = Box::new(renderer);
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        bus.publish(Event::TaskStarted {
            metadata: TaskMetadata::builder().id(1).name("task1").build(),
        })
        .unwrap();
        {
            let mut scheduler = ui.lock().unwrap();
            scheduler.skip_render_interval();
        }
        let is_rendered = {
            let mut scheduler = ui.lock().unwrap();
            scheduler.tick_render().unwrap()
        };
        assert!(
            is_rendered,
            "tick_render should return true when all conditions are met"
        );
        assert_eq!(*running_calls.lock().unwrap(), 1);
    }

    #[test]
    fn block_on_task_thread_waits_for_success() {
        let bus = MockEventBus::default();
        let renderer = Box::new(MockRenderer::default());
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        let (tx, rx) = mpsc::channel::<Result<()>>();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            let _ = tx.send(Ok(()));
        });
        let result = ui.block_on_task_thread_finish_channel(&rx);
        assert!(result.is_ok());
    }

    #[test]
    fn block_on_task_thread_propagates_panic_as_error() {
        let bus = MockEventBus::default();
        let renderer = Box::new(MockRenderer::default());
        let ui = SyncUi::bind_event_bus(renderer, &bus).unwrap();
        let (_, rx) = mpsc::channel::<Result<()>>();
        thread::spawn(move || {
            panic!("intentional test panic");
        });
        let err = ui.block_on_task_thread_finish_channel(&rx).unwrap_err();
        assert_debug_snapshot!(err,@r#""Task execution thread panicked or exited unexpectedly""#);
    }
}
