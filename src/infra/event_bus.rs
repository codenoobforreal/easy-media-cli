use crate::domain::event::{Event, EventBus, EventHandler};
use anyhow::{Result, anyhow};
use std::sync::Mutex;

pub type HandlerStorage = Mutex<Vec<EventHandler>>;

#[derive(Default)]
pub struct DefaultEventBus {
    handlers: HandlerStorage,
}

impl EventBus for DefaultEventBus {
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
#[allow(clippy::similar_names)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_publish_should_deliver_to_all_subscribers() -> Result<()> {
        let bus = DefaultEventBus::default();
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let handler1: EventHandler = Arc::new(move |_| {
            *call_count_clone.lock().unwrap() += 1;
            Ok(())
        });

        let call_count2 = Arc::new(Mutex::new(0));
        let call_count2_clone = call_count2.clone();
        let handler2: EventHandler = Arc::new(move |_| {
            *call_count2_clone.lock().unwrap() += 1;
            Ok(())
        });

        bus.subscribe(handler1)?;
        bus.subscribe(handler2)?;

        bus.publish(Event::Shutdown)?;

        assert_eq!(*call_count.lock().unwrap(), 1);
        assert_eq!(*call_count2.lock().unwrap(), 1);
        Ok(())
    }

    #[test]
    fn test_publish_should_ignore_subscriber_errors() -> Result<()> {
        let bus = DefaultEventBus::default();
        let error_handler: EventHandler = Arc::new(|_| Err(anyhow::anyhow!("handler error")));
        let ok_handler: EventHandler = Arc::new(|_| Ok(()));

        bus.subscribe(error_handler)?;
        bus.subscribe(ok_handler)?;

        let result = bus.publish(Event::Shutdown);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_publish_critical_should_stop_on_first_error() -> Result<()> {
        let bus = DefaultEventBus::default();
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let error_handler: EventHandler = Arc::new(move |_| {
            *call_count_clone.lock().unwrap() += 1;
            Err(anyhow::anyhow!("handler error"))
        });

        let ok_handler: EventHandler = Arc::new(|_| Ok(()));

        bus.subscribe(error_handler)?;
        bus.subscribe(ok_handler)?;

        let result = bus.publish_critical(Event::Shutdown);
        assert!(result.is_err());
        assert_eq!(*call_count.lock().unwrap(), 1);
        Ok(())
    }
}
