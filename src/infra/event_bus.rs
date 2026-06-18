use crate::domain::Event;
use anyhow::{Context, Result, anyhow};
use std::sync::Mutex;

/// 事件处理函数类型：接收Event，返回错误/空结果，支持跨线程传递
pub type EventHandler = Box<dyn Fn(Event) -> Result<()> + Send + Sync>;
/// 存储全部订阅处理器的线程安全容器
pub type HandlerStorage = Mutex<Vec<EventHandler>>;

pub trait EventBus: Send + Sync {
    fn publish(&self, event: Event) -> Result<()>;
    fn subscribe(&self, handler: EventHandler) -> Result<()>;
}

#[derive(Default)]
pub struct DefaultEventBus {
    handlers: HandlerStorage,
}

impl EventBus for DefaultEventBus {
    fn publish(&self, event: Event) -> Result<()> {
        let handlers_guard = self
            .handlers
            .lock()
            .map_err(|e| anyhow!("handlers lock mutex poisoned: {e}"))?;
        for handler in handlers_guard.iter() {
            handler(event.clone()).with_context(|| "Failed to run handler".to_owned())?;
        }
        drop(handlers_guard);

        Ok(())
    }

    fn subscribe(&self, handler: Box<dyn Fn(Event) -> Result<()> + Send + Sync>) -> Result<()> {
        let mut handlers_guard = self
            .handlers
            .lock()
            .map_err(|e| anyhow!("handlers lock mutex poisoned: {e}"))?;
        handlers_guard.push(handler);
        drop(handlers_guard);

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::domain::sample_test_metadata;
    use insta::assert_debug_snapshot;
    use std::sync::Arc;

    #[derive(Default)]
    pub struct MockEventBus {
        events: Mutex<Vec<Event>>,
        handlers: Mutex<Vec<EventHandler>>,
    }

    impl MockEventBus {
        pub fn events(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventBus for MockEventBus {
        fn publish(&self, event: Event) -> Result<()> {
            // 先执行所有订阅者回调，与真实总线行为一致
            let handlers = self.handlers.lock().unwrap();
            for handler in handlers.iter() {
                handler(event.clone())?;
            }
            drop(handlers);

            // 保留原有的事件收集能力，用于测试断言
            self.events.lock().unwrap().push(event);
            Ok(())
        }

        fn subscribe(&self, handler: EventHandler) -> Result<()> {
            // 真正保存订阅处理器，publish 时依次调用
            self.handlers.lock().unwrap().push(handler);
            Ok(())
        }
    }

    #[test]
    fn publish_with_no_subscribers_returns_ok() {
        let bus = DefaultEventBus::default();
        let result = bus.publish(Event::TaskStarted {
            metadata: sample_test_metadata(1),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn subscriber_receives_published_event() {
        let bus = DefaultEventBus::default();
        let received = Arc::new(Mutex::new(vec![]));
        let received_clone = received.clone();
        bus.subscribe(Box::new(move |event| {
            received_clone.lock().unwrap().push(event);
            Ok(())
        }))
        .unwrap();
        bus.publish(Event::TaskStarted {
            metadata: sample_test_metadata(42),
        })
        .unwrap();
        let events = received.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_debug_snapshot!(&events[0],@r#"
        TaskStarted {
            metadata: TaskMetadata {
                id: 42,
                name: "sample_task_42",
                status: Pending,
                progress: None,
                error: None,
                result: None,
            },
        }
        "#);
    }

    #[test]
    fn multiple_subscribers_all_receive_event() {
        let bus = DefaultEventBus::default();
        let count1 = Arc::new(Mutex::new(0));
        let count2 = Arc::new(Mutex::new(0));
        let c1 = count1.clone();
        bus.subscribe(Box::new(move |_| {
            *c1.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        let c2 = count2.clone();
        bus.subscribe(Box::new(move |_| {
            *c2.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        bus.publish(Event::TaskStarted {
            metadata: sample_test_metadata(1),
        })
        .unwrap();
        assert_eq!(*count1.lock().unwrap(), 1);
        assert_eq!(*count2.lock().unwrap(), 1);
    }

    #[test]
    fn subscriber_error_propagates_to_publish() {
        let bus = DefaultEventBus::default();
        bus.subscribe(Box::new(|_| Err(anyhow!("Handler failed"))))
            .unwrap();
        let err = bus
            .publish(Event::TaskStarted {
                metadata: sample_test_metadata(1),
            })
            .unwrap_err();
        assert_debug_snapshot!(err,@r#"
        Error {
            context: "Failed to run handler",
            source: "Handler failed",
        }
        "#);
    }

    #[test]
    fn mock_bus_collects_all_published_events() {
        let bus = MockEventBus::default();
        bus.publish(Event::TaskStarted {
            metadata: sample_test_metadata(1),
        })
        .unwrap();
        bus.publish(Event::TaskCompleted { id: 1 }).unwrap();
        assert_eq!(bus.events().len(), 2);
    }

    #[test]
    fn mock_bus_subscribe_receives_all_published_events() {
        let bus = MockEventBus::default();
        let count = Arc::new(Mutex::new(0));
        let count_clone = count.clone();
        bus.subscribe(Box::new(move |_| {
            *count_clone.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        bus.publish(Event::Shutdown).unwrap();
        bus.publish(Event::Shutdown).unwrap();
        assert_eq!(*count.lock().unwrap(), 2);
        assert_eq!(bus.events().len(), 2);
    }
}
