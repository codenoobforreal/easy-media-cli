use std::io;
use thiserror::Error;
use tokio::{sync::AcquireError, task};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Handle join error: {0}")]
    HandleJoinError(#[from] task::JoinError),

    #[error("AcquireError error: {0}")]
    AcquireError(#[from] AcquireError),

    #[error("Command line error: {0}")]
    Cli(String),

    #[error("Task execution failed: {0}")]
    TaskFailed(String),

    #[error("FFmpeg error: {0}")]
    FfmpegError(String),

    #[error("UI error: {0}")]
    UiError(String),

    #[error("Event bus error: {0}")]
    EventBusError(String),
}

pub type AppResult<T> = Result<T, AppError>;
