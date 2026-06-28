//! `FfmpegTaskWrapper` + 通用执行流程

use crate::{
    common::join_errors_with_summary,
    domain::{
        cancel_token::CancelToken,
        event::{Event, EventBus, TaskResultPayload},
        task::{Task, TaskError},
    },
    infra::{
        CapturingCommandRunner, CapturingCommandRunnerExt, ChildGuard, FfmpegProgressParser,
        FileSystem, ProgressTracker, StreamingCommandRunnerExt,
    },
    task::command::{CommandTask, ExecutionMode},
};
use anyhow::{Context, Result, anyhow};
use std::{
    io::{BufRead, BufReader, Read},
    ops::Mul,
    process::ExitStatus,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// `IO` 线程句柄集合，简化参数传递
/// - 脱离 `FFmpeg` 视频任务执行场景，这个结构体没有任何复用价值，目前暂时放置在这里
/// - 未来有多场景复用的需要，可以在 `task/ffmpeg/mod.rs` 内部定义为 `pub(crate) struct IoThreadHandles`，仅整个 `ffmpeg` 任务子模块内部共享
struct IoThreadHandles {
    stdout: thread::JoinHandle<Result<Option<u64>>>,
    stderr: thread::JoinHandle<Result<Vec<u8>>>,
}

/// 命令任务包装器
/// - 组合了「具体业务任务」和「通用依赖」
/// - 给包装器实现通用 `Task` 接口，才能被 `Runner` 当作通用 `Task` 执行
#[derive(Debug)]
pub struct CommandTaskWrapper<T: CommandTask> {
    /// 嵌套的具体任务：Thumbnail、Transcode、ExtractAudio 等
    inner: T,
    // 通用依赖统一由包装层持有，业务任务完全不感知
    command_runner: Arc<dyn CapturingCommandRunner>,
    file_system: Arc<dyn FileSystem>,
    render_interval: Duration,
    progress_threshold: f32,
}

impl<T: CommandTask> Task for CommandTaskWrapper<T> {
    fn id(&self) -> usize {
        self.inner.id()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn run(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<Option<TaskResultPayload>, TaskError> {
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        match self.inner.execution_mode() {
            ExecutionMode::Streaming => self.run_streaming_mode(event_bus, cancel_token),
            ExecutionMode::Capturing => self.run_capturing_mode(cancel_token),
        }
    }
}

impl<T: CommandTask> CommandTaskWrapper<T> {
    pub fn new(
        inner: T,
        command_runner: Arc<dyn CapturingCommandRunner>,
        file_system: Arc<dyn FileSystem>,
        render_interval: Duration,
        progress_threshold: f32,
    ) -> Self {
        Self {
            inner,
            command_runner,
            file_system,
            render_interval,
            progress_threshold,
        }
    }

    fn run_streaming_mode(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<Option<TaskResultPayload>, TaskError> {
        if self.inner.needs_output_dir() {
            let output = self.inner.output();
            let output_dir = output
                .and_then(|o| o.parent())
                .with_context(|| format!("Failed to get parent path of {output:?}"))?;
            self.file_system
                .create_dir_all(output_dir)
                .with_context(|| format!("Failed to create dir for {}", output_dir.display()))?;
        }

        let total_duration = self
            .inner
            .duration()
            .with_context(|| "Failed to retrive duration")?;

        let start_time = Instant::now();

        let (mut child_guard, io_handles) = self.spawn_with_io_handlers(
            event_bus,
            total_duration,
            start_time,
            self.render_interval,
            self.progress_threshold,
        )?;

        let exit_status =
            Self::wait_for_completion(&mut child_guard, cancel_token, self.render_interval)?;
        let total_size = self.finalize_streaming_result(exit_status, io_handles, cancel_token)?;

        Ok(self.inner.result_payload(total_size))
    }

    fn run_capturing_mode(
        &self,
        cancel_token: &dyn CancelToken,
    ) -> Result<Option<TaskResultPayload>, TaskError> {
        // 前置检查：启动前先判断是否已取消
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        let spec = self.inner.command_spec();
        let output = self
            .command_runner
            .run_and_capture(&spec.program, &spec.args)?;

        // 执行后检查取消（虽然中途不能取消，但结束后统一判断语义）
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        if let Some(payload) = self
            .inner
            .handle_captured_output(&output.stdout, &output.stderr)?
        {
            return Ok(Some(payload));
        }

        if !output.status.success() {
            let err = match output.status.code() {
                Some(code) => anyhow!("FFmpeg exited with non-zero exit code: {code}"),
                None => anyhow!("FFmpeg process terminated by signal/crash, no exit code"),
            };
            return Err(TaskError::Failed(err));
        }

        // 如果捕获模式任务还需要更多数据（比如文件大小），可以在这里调用 result_payload。
        Ok(None)
    }

    /// 启动子进程，并根据配置启动对应的 `IO` 处理线程
    fn spawn_with_io_handlers(
        &self,
        event_bus: &Arc<dyn EventBus>,
        total_duration: Duration,
        start_time: Instant,
        render_interval: Duration,
        progress_threshold: f32,
    ) -> Result<(ChildGuard, IoThreadHandles)> {
        let spec = self.inner.command_spec();
        let command_streams = self.command_runner.spawn(&spec.program, &spec.args)?;
        let child_guard = ChildGuard::new(command_streams.child_handle);

        let stderr_handle = thread::spawn(move || Self::read_stderr(command_streams.stderr));

        let id = self.id();
        let stdout_handle = if self.inner.needs_progress() {
            let event_bus_clone = event_bus.clone();
            thread::spawn(move || {
                read_progress(
                    id,
                    event_bus_clone.as_ref(),
                    command_streams.stdout,
                    start_time,
                    total_duration,
                    render_interval,
                    progress_threshold,
                )
            })
        } else {
            thread::spawn(move || Self::drain_stdout(command_streams.stdout).and(Ok(None)))
        };

        Ok((
            child_guard,
            IoThreadHandles {
                stdout: stdout_handle,
                stderr: stderr_handle,
            },
        ))
    }

    /// 自适应间隔轮询进程状态与取消信号，返回最终退出状态
    fn wait_for_completion(
        child_guard: &mut ChildGuard,
        cancel_token: &dyn CancelToken,
        render_interval: Duration,
    ) -> Result<ExitStatus> {
        // 配置轮询间隔策略
        const QUICK_CHECKS: u32 = 4;

        let mut sleep_duration = Duration::from_millis(10);
        let mut checks = 0;

        loop {
            if cancel_token.is_cancelled() {
                child_guard.kill()?;
                let status = child_guard.wait()?;
                return Ok(status);
            }

            if let Some(status) = child_guard.try_wait()? {
                return Ok(status);
            }

            thread::sleep(sleep_duration);
            checks += 1;
            if checks > QUICK_CHECKS {
                sleep_duration = (sleep_duration * 2).min(render_interval);
            }
        }
    }

    /// 等待 `IO` 线程结束，收集错误，区分取消与业务失败
    fn finalize_streaming_result(
        &self,
        exit_status: ExitStatus,
        io_handles: IoThreadHandles,
        cancel_token: &dyn CancelToken,
    ) -> Result<Option<u64>, TaskError> {
        let stdout_res = io_handles
            .stdout
            .join()
            .map_err(|_| anyhow!("Stdout processing thread panicked"))?;
        let stderr_res = io_handles
            .stderr
            .join()
            .map_err(|_| anyhow!("Stderr processing thread panicked"))?;

        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        let mut errors = Vec::new();
        let output_size = match stdout_res {
            Ok(size) => size,
            Err(e) => {
                errors.push(e);
                None
            }
        };
        let stderr_bytes = match stderr_res {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(e);
                Vec::new()
            }
        };

        if !exit_status.success() {
            let mut exit_err = match exit_status.code() {
                Some(code) => anyhow!("FFmpeg exited with non-zero exit code: {code}"),
                None => anyhow!("FFmpeg process terminated by signal/crash, no exit code"),
            };
            if !stderr_bytes.is_empty() {
                let stderr_text = String::from_utf8_lossy(&stderr_bytes);
                exit_err = exit_err.context(format!("stderr:\n{stderr_text}"));
            }
            errors.push(exit_err);
        }

        if errors.is_empty() {
            Ok(output_size)
        } else {
            let summary = format!("Task {} failed with {} errors", self.id(), errors.len());
            Err(TaskError::Failed(join_errors_with_summary(
                summary, &errors,
            )))
        }
    }

    /// 排空 `stdout`，不做处理（无进度任务用，避免管道阻塞）
    fn drain_stdout(mut reader: impl Read) -> Result<()> {
        let mut buf = [0u8; 8 * 1024];
        while reader.read(&mut buf)? > 0 {}
        Ok(())
    }

    /// 收集 `stderr` 原始字节，不进行编码转换或内容判定
    fn read_stderr(stderr_reader: impl Read) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        BufReader::new(stderr_reader)
            .read_to_end(&mut buf)
            .with_context(|| "Failed to read stderr")?;

        Ok(buf)
    }
}

/// 解析 `stdout` 进度并发布事件（有进度任务用）
pub fn read_progress(
    id: usize,
    event_bus: &dyn EventBus,
    stdout_reader: impl Read,
    start_time: Instant,
    total_duration: Duration,
    render_interval: Duration,
    progress_threshold: f32,
) -> Result<Option<u64>> {
    let mut buf_reader = BufReader::new(stdout_reader);
    let mut parser = FfmpegProgressParser::default();
    let mut tracker = ProgressTracker::new(total_duration, progress_threshold);
    let mut last_publish = Instant::now()
        .checked_sub(render_interval.mul(2))
        .unwrap_or(Instant::now());

    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = buf_reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }

        let trimmed = line.trim_end_matches(['\n', '\r']);
        let Some(raw_progress) = parser.feed_line(trimmed)? else {
            continue;
        };

        let now = Instant::now();
        let elapsed = now - start_time;
        let time_since_last_publish = now - last_publish;
        if time_since_last_publish >= render_interval
            && let Some(progress) = tracker.update(raw_progress, elapsed)
        {
            event_bus.publish(Event::TaskProgress { id, progress })?;
            last_publish = now;
        }
    }

    Ok(parser.total_size())
}
