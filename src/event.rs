use crate::task::Progress;
use anyhow::{Result, anyhow};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum Event {
    TaskStarted { id: u64 },
    TaskProgress { id: u64, progress: Progress },
    TaskCompleted { id: u64 },
    TaskFailed { id: u64, error: String },
    AllTasksCompleted,
    Shutdown,
}

impl Event {
    pub fn get_id(&self) -> u64 {
        match self {
            Event::TaskStarted { id } => *id,
            Event::TaskProgress { id, .. } => *id,
            Event::TaskCompleted { id } => *id,
            Event::TaskFailed { id, .. } => *id,
            _ => 0,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: Event) -> Result<()> {
        self.sender
            .send(event.clone())
            .map_err(|e| anyhow!("Failed to publish [{event:?}]: {}", e))?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}
