mod info;
mod progress;
mod status;
mod thumbnail;
mod transcode;

use crate::{error::AppResult, event::EventBus};
use async_trait::async_trait;
use std::{fmt, sync::Arc};

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
    async fn run(&self, event_bus: EventBus) -> AppResult<()>;
}

pub type SharedTask = Arc<dyn Task>;
