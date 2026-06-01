mod id_generator;
mod manager;
mod matadata;
mod scene_cut_snap;

pub use id_generator::IdGenerator;
pub use manager::{Manager, NotifyEvent, RegistryMap};
pub use matadata::{Metadata, MetadataMap};
pub use scene_cut_snap::SceneCutSnapTask;

use crate::{client::Client, progress::Progress};
use anyhow::Result;
use std::{fmt, sync::mpsc::Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    SceneCutSnap,
    Av1Transcode,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display_name = match self {
            Type::SceneCutSnap => "Scene Cut Snapshot",
            Type::Av1Transcode => "AV1 Transcode",
        };
        write!(f, "{}", display_name)
    }
}

pub trait Task: Send {
    fn task_type(&self) -> Type;
    fn supports_progress(&self) -> bool {
        false
    }
    fn execute(&self, client: Box<dyn Client>) -> Result<()>;
    fn execute_with_progress(
        &self,
        client: Box<dyn Client>,
        progress_sender: Sender<Progress>,
    ) -> Result<()>;
}
