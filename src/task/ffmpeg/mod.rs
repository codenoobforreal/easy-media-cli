//! `FFmpeg` 任务通用执行框架

mod wrapper;

use anyhow::Result;
use std::{
    ffi::{OsStr, OsString},
    path::Path,
};
pub use wrapper::{FfmpegTaskWrapper, read_progress_impl};

/// 领域 trait
pub trait FfmpegTask: Send + Sync {
    fn id(&self) -> usize;
    fn name(&self) -> Option<&str>;
    fn input(&self) -> &Path;
    fn output(&self) -> Option<&Path>;
    fn build_args(&self) -> Vec<OsString>;
    fn file_name(&self) -> Option<&OsStr>;

    /// 是否需要解析进度并发布事件，默认开启，不需要进度的任务可重写返回 false
    fn needs_progress(&self) -> bool {
        true
    }

    /// 任务执行模式，默认流式执行
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Streaming
    }

    /// 捕获模式下处理命令输出（比如解析 ffprobe JSON），默认空实现，需要解析输出的任务可重写
    fn handle_captured_output(&self, _stdout: &[u8], _stderr: &[u8]) -> Result<()> {
        Ok(())
    }

    /// 当前任务是否需要提前创建输出目录
    fn needs_output_dir(&self) -> bool {
        self.output().is_some()
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
