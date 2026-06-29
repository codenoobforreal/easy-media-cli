use crate::{
    common::{format_duration_all, human_readable_size},
    domain::{media::MediaMetadata, progress::Progress, task::TaskMetadata},
};
use anyhow::Result;
use std::{fmt, path::PathBuf, sync::Arc, time::Duration};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskResultPayload {
    VideoEncoder {
        output_path: PathBuf,
        size_bytes: u64,
        size_change: f64,
        duration: Duration,
    },

    ThumbnailGenerator {
        output_dir: PathBuf,
        duration: Duration,
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
                size_change,
                duration,
            } => {
                let change = if size_change.is_sign_positive() {
                    format!("{:.2}% bigger", size_change.abs() * 100.0)
                } else {
                    format!("{:.2}% smaller", size_change.abs() * 100.0)
                };

                write!(
                    f,
                    "Output: {} ({} | {change} | {})",
                    output_path.display(),
                    human_readable_size(*size_bytes),
                    format_duration_all(*duration)
                )
            }

            TaskResultPayload::ThumbnailGenerator {
                output_dir,
                duration,
            } => {
                write!(
                    f,
                    "Thumbnails saved in: {} | {}",
                    output_dir.display(),
                    format_duration_all(*duration)
                )
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
