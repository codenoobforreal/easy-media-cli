//! 任务调度层：通用任务调度与执行框架

mod ffmpeg;
mod manager;

pub use ffmpeg::{ExecutionMode, FfmpegTask, FfmpegTaskWrapper};
pub use manager::TaskManager;
