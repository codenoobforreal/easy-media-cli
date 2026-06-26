//! `FFmpeg` 任务通用执行框架

mod wrapper;

use anyhow::Result;
use std::{
    ffi::{OsStr, OsString},
    path::Path,
    time::Duration,
};
pub use wrapper::{FfmpegTaskWrapper, read_progress};

pub trait FfmpegTask: Send + Sync {
    fn id(&self) -> usize;
    fn name(&self) -> Option<&str>;
    fn input(&self) -> &Path;
    fn output(&self) -> Option<&Path>;
    fn build_args(&self) -> Vec<OsString>;
    fn file_name(&self) -> Option<&OsStr>;

    /// 是否需要解析进度并发布事件，默认开启
    fn needs_progress(&self) -> bool {
        true
    }

    /// 任务执行模式，默认流式执行
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Streaming
    }

    /// 捕获模式下处理命令输出（比如解析 ffprobe JSON），默认空实现
    fn handle_captured_output(&self, _stdout: &[u8], _stderr: &[u8]) -> Result<()> {
        Ok(())
    }

    /// 当前任务是否需要提前创建输出目录，根据 output 是否有值判断
    fn needs_output_dir(&self) -> bool {
        self.output().is_some()
    }

    /// 任务已预知的视频时长，用于进度计算。返回 `Some` 可避免包装器重复获取元数据。
    fn duration(&self) -> Option<Duration> {
        None
    }
}

/// `FFmpeg` 类任务执行模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// 流式执行：边运行边读输出，支持实时进度、中途取消，适合长耗时任务
    Streaming,
    /// 捕获执行：等进程结束一次性拿全部输出，不支持中途取消，适合短平快任务
    Capturing,
}
