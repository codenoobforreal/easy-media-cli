use crate::{
    common::human_readable_size,
    domain::{Metadata, TaskMetadata},
    infra::Progress,
};
use std::{fmt, path::PathBuf};

#[derive(Debug, Clone)]
pub enum TaskResultPayload {
    VideoEncoder {
        output_path: PathBuf,
        size_bytes: Option<u64>,
    },
    ThumbnailGenerator {
        output_dir: PathBuf,
    },

    MediaMetadataGetter {
        metadata: Metadata,
    },
}

impl TaskResultPayload {
    pub fn summary(&self) -> String {
        match self {
            TaskResultPayload::VideoEncoder {
                output_path,
                size_bytes,
            } => {
                format!(
                    "Output: {} ({} bytes)",
                    output_path.display(),
                    size_bytes.unwrap_or_default()
                )
            }

            TaskResultPayload::ThumbnailGenerator { output_dir } => {
                format!("Generated thumbnails in {}", output_dir.display())
            }

            TaskResultPayload::MediaMetadataGetter { metadata } => {
                format!("{metadata:?}")
            }
        }
    }
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
