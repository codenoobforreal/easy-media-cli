mod info;
mod progress;
mod status;
mod thumbnail;
mod transcode;

use crate::event::EventBus;
use anyhow::Result;
use async_trait::async_trait;
use std::{fmt, sync::Arc};
use tokio_util::sync::CancellationToken;

pub use info::Info;
pub use progress::Progress;
pub use status::Status;
pub use thumbnail::Thumbnail;

#[async_trait]
pub trait Task: Send + Sync + fmt::Debug {
    fn id(&self) -> u64;
    fn name(&self) -> &str;
    fn file_path(&self) -> Option<&str> {
        None
    }
    fn file_name(&self) -> Option<&str> {
        None
    }
    async fn run(&self, event_bus: EventBus, cancel_token: CancellationToken) -> Result<()>;
}

pub type SharedTask = Arc<dyn Task>;
