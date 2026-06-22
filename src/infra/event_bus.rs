use crate::domain::Event;
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};

pub type EventHandler = Arc<dyn Fn(Event) -> Result<()> + Send + Sync>;
pub type HandlerStorage = Mutex<Vec<EventHandler>>;

pub trait EventBus: Send + Sync {
    /// 尽力交付事件给所有订阅者，单个订阅者的错误会被忽略（后续可添加记录），不会中断发布流程
    fn publish(&self, event: Event) -> Result<()>;
    /// 任一订阅者失败则立即返回错误，后续订阅者不会收到事件
    fn publish_critical(&self, event: Event) -> Result<()>;
    fn subscribe(&self, handler: EventHandler) -> Result<()>;
}

#[derive(Default)]
pub struct DefaultEventBus {
    handlers: HandlerStorage,
}

impl EventBus for DefaultEventBus {
    /// 使用克隆处理器的实现，避免锁竞争
    fn publish(&self, event: Event) -> Result<()> {
        let handlers_snapshot = {
            self.handlers
                .lock()
                .map_err(|e| anyhow!("handlers lock mutex poisoned: {e}"))?
                .clone()
        };

        for handler in handlers_snapshot {
            let _ = handler(event.clone());
        }

        Ok(())
    }

    fn publish_critical(&self, event: Event) -> Result<()> {
        let handlers_snapshot = {
            self.handlers
                .lock()
                .map_err(|e| anyhow!("handlers lock mutex poisoned: {e}"))?
                .clone()
        };

        for handler in handlers_snapshot {
            handler(event.clone())?;
        }

        Ok(())
    }

    fn subscribe(&self, handler: EventHandler) -> Result<()> {
        let mut handlers_guard = self
            .handlers
            .lock()
            .map_err(|e| anyhow!("handlers lock mutex poisoned: {e}"))?;
        handlers_guard.push(handler);

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::domain::sample_test_metadata;
    use insta::assert_debug_snapshot;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::Arc,
    };

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
            self.events.lock().unwrap().push(event.clone());
            let handlers = self.handlers.lock().unwrap();
            for handler in handlers.iter() {
                let _ = handler(event.clone());
            }
            drop(handlers);

            Ok(())
        }

        fn publish_critical(&self, event: Event) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            let handlers = self.handlers.lock().unwrap();
            for handler in handlers.iter() {
                handler(event.clone())?;
            }
            drop(handlers);

            Ok(())
        }

        fn subscribe(&self, handler: EventHandler) -> Result<()> {
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
        bus.subscribe(Arc::new(move |event| {
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
        bus.subscribe(Arc::new(move |_| {
            *c1.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        let c2 = count2.clone();
        bus.subscribe(Arc::new(move |_| {
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
    fn publish_critical_propagates_handler_error() {
        let bus = DefaultEventBus::default();
        bus.subscribe(Arc::new(|_| Err(anyhow!("Handler failed"))))
            .unwrap();
        let err = bus
            .publish_critical(Event::TaskStarted {
                metadata: sample_test_metadata(1),
            })
            .unwrap_err();
        assert_debug_snapshot!(err,@r#""Handler failed""#);
    }

    #[test]
    fn publish_critical_stops_on_first_error() {
        let bus = DefaultEventBus::default();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        bus.subscribe(Arc::new(|_| Err(anyhow!("Failed")))).unwrap();
        bus.subscribe(Arc::new(move |_| {
            *counter_clone.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        let err = bus.publish_critical(Event::Shutdown).unwrap_err();
        assert_debug_snapshot!(err,@r#""Failed""#);
        // assert!(err.to_string().contains("Failed"));
        assert_eq!(*counter.lock().unwrap(), 0);
    }

    #[test]
    fn publish_ignores_handler_error() {
        let bus = DefaultEventBus::default();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();
        bus.subscribe(Arc::new(|_| Err(anyhow!("Failed")))).unwrap();
        bus.subscribe(Arc::new(move |_| {
            *counter_clone.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        let result = bus.publish(Event::Shutdown);
        assert!(result.is_ok());
        assert_eq!(*counter.lock().unwrap(), 1);
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
        bus.subscribe(Arc::new(move |_| {
            *count_clone.lock().unwrap() += 1;
            Ok(())
        }))
        .unwrap();
        bus.publish(Event::Shutdown).unwrap();
        bus.publish(Event::Shutdown).unwrap();
        assert_eq!(*count.lock().unwrap(), 2);
        assert_eq!(bus.events().len(), 2);
    }

    #[test]
    fn mock_publish_ignores_handler_error() {
        let bus = MockEventBus::default();
        bus.subscribe(Arc::new(|_| Err(anyhow!("oops")))).unwrap();
        assert!(bus.publish(Event::Shutdown).is_ok());
        assert_eq!(bus.events().len(), 1);
    }

    #[test]
    fn mock_publish_critical_propagates_error() {
        let bus = MockEventBus::default();
        bus.subscribe(Arc::new(|_| Err(anyhow!("oops")))).unwrap();
        assert!(bus.publish_critical(Event::Shutdown).is_err());
        assert!(!bus.events().is_empty());
    }

    #[test]
    fn publish_returns_error_on_poisoned_mutex() {
        let bus = DefaultEventBus::default();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = bus.handlers.lock().unwrap();
            panic!("intentional panic to poison mutex");
        }));
        assert!(result.is_err());
        let err = bus.publish(Event::Shutdown).unwrap_err();
        assert_debug_snapshot!(err,@r#""handlers lock mutex poisoned: poisoned lock: another task failed inside""#);
    }

    #[test]
    fn publish_critical_returns_error_on_poisoned_mutex() {
        let bus = DefaultEventBus::default();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = bus.handlers.lock().unwrap();
            panic!("intentional panic to poison mutex");
        }));
        assert!(result.is_err());
        let err = bus.publish_critical(Event::Shutdown).unwrap_err();
        assert_debug_snapshot!(err,@r#""handlers lock mutex poisoned: poisoned lock: another task failed inside""#);
    }
}
