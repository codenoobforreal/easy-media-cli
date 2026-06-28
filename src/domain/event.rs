use crate::{
    common::human_readable_size,
    domain::{media::MediaMetadata, progress::Progress, task::TaskMetadata},
};
use anyhow::Result;
use std::{fmt, path::PathBuf, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskResultPayload {
    VideoEncoder {
        output_path: PathBuf,
        size_bytes: Option<u64>,
    },
    ThumbnailGenerator {
        output_dir: PathBuf,
    },

    MediaMetadataGetter {
        metadata: MediaMetadata,
    },
}

impl fmt::Display for TaskResultPayload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TaskResultPayload::VideoEncoder {
                output_path,
                size_bytes,
            } => {
                write!(
                    f,
                    "Encoded video: {} ({})",
                    output_path.display(),
                    human_readable_size(size_bytes.unwrap_or_default())
                )
            }

            TaskResultPayload::ThumbnailGenerator { output_dir } => {
                write!(f, "Thumbnails saved in: {}", output_dir.display())
            }

            TaskResultPayload::MediaMetadataGetter { metadata } => {
                write!(f, "{metadata}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    TaskQueueStart {
        total: usize,
    },

    TaskStarted {
        metadata: TaskMetadata,
    },

    TaskProgress {
        id: usize,
        progress: Progress,
    },

    TaskCompleted {
        id: usize,
        payload: Option<TaskResultPayload>,
    },

    TaskFailed {
        id: usize,
        error: String,
    },

    TaskCancelled {
        id: usize,
    },

    AllTasksCompleted {
        total: usize,
        success: usize,
        failed: usize,
        cancelled: usize,
    },

    Shutdown,
}

pub type EventHandler = Arc<dyn Fn(Event) -> Result<()> + Send + Sync>;

/// 同步发布订阅
pub trait EventBus: Send + Sync {
    /// 尽力交付事件给所有订阅者，单个订阅者的错误会被忽略（后续可添加记录），不会中断发布流程
    fn publish(&self, event: Event) -> Result<()>;
    /// 任一订阅者失败则立即返回错误，后续订阅者不会收到事件
    fn publish_critical(&self, event: Event) -> Result<()>;
    fn subscribe(&self, handler: EventHandler) -> Result<()>;
}
