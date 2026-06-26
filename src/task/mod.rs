//! 任务编排层
//! 通用任务调度、生命周期管理、通用任务包装
//! 依赖领域层契约

mod ffmpeg;
mod manager;

pub use ffmpeg::{ExecutionMode, FfmpegTask, FfmpegTaskWrapper, read_progress};
pub use manager::TaskManager;
