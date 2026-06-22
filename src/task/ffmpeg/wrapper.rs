//! `FfmpegTaskWrapper` + 通用执行流程

use crate::{
    common::join_errors_with_summary,
    domain::{Event, Task, TaskError},
    ffmpeg_progress::{FfmpegProgressParser, ProgressTracker},
    infra::{
        CancelToken, CapturingCommandRunner, CapturingCommandRunnerExt, ChildGuard, EventBus,
        FileSystem, StreamingCommandRunnerExt,
    },
    media_metadata::MetadataFetcher,
    task::{ExecutionMode, ffmpeg::FfmpegTask},
};
use anyhow::{Context, Result, anyhow};
use std::{
    io::{BufRead, BufReader, Read},
    process::ExitStatus,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// `IO` 线程句柄集合，简化参数传递
/// - 脱离 `FFmpeg` 视频任务执行场景，这个结构体没有任何复用价值，目前暂时放置在这里
/// - 未来有多场景复用的需要，可以在 `task/ffmpeg/mod.rs` 内部定义为 `pub(crate) struct IoThreadHandles`，仅整个 `ffmpeg` 任务子模块内部共享
struct IoThreadHandles {
    stdout: thread::JoinHandle<Result<()>>,
    stderr: thread::JoinHandle<Result<Vec<u8>>>,
}

/// `FFmpeg` 任务包装器
/// - 组合了「具体业务任务」和「通用依赖」
/// - 给包装器实现通用 `Task` 接口，才能被 `Runner` 当作通用 `Task` 执行
pub struct FfmpegTaskWrapper<T: FfmpegTask> {
    /// 嵌套的具体任务：Thumbnail、Transcode、ExtractAudio 等
    inner: T,
    // 通用依赖统一由包装层持有，业务任务完全不感知
    command_runner: Arc<dyn CapturingCommandRunner>,
    metadata_fetcher: Arc<dyn MetadataFetcher>,
    file_system: Arc<dyn FileSystem>,
}

impl<T: FfmpegTask> Task for FfmpegTaskWrapper<T> {
    fn id(&self) -> usize {
        self.inner.id()
    }

    fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    fn run(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<(), TaskError> {
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
        match self.inner.execution_mode() {
            ExecutionMode::Streaming => self.run_streaming_mode(event_bus, cancel_token),
            ExecutionMode::Capturing => self.run_capturing_mode(cancel_token),
        }
    }
}

impl<T: FfmpegTask> FfmpegTaskWrapper<T> {
    pub fn new(
        inner: T,
        command_runner: Arc<dyn CapturingCommandRunner>,
        metadata_fetcher: Arc<dyn MetadataFetcher>,
        file_system: Arc<dyn FileSystem>,
    ) -> Self {
        Self {
            inner,
            command_runner,
            metadata_fetcher,
            file_system,
        }
    }

    fn run_streaming_mode(
        &self,
        event_bus: &Arc<dyn EventBus>,
        cancel_token: &dyn CancelToken,
    ) -> Result<(), TaskError> {
        if self.inner.needs_output_dir() {
            let output = self.inner.output();
            let output_dir = output
                .and_then(|o| o.parent())
                .with_context(|| format!("Failed to get parent path of {output:?}"))?;
            self.file_system
                .create_dir_all(output_dir)
                .with_context(|| format!("Failed to create dir for {}", output_dir.display()))?;
        }

        let metadata = self.metadata_fetcher.fetch_metadata(self.inner.input())?;
        let total_duration = metadata.duration();
        let start_time = Instant::now();

        let (mut child_guard, io_handles) =
            self.spawn_with_io_handlers(event_bus, total_duration, start_time)?;

        let exit_status = Self::wait_for_completion(&mut child_guard, cancel_token)?;

        self.finalize_streaming_result(exit_status, io_handles, cancel_token)
    }

    fn run_capturing_mode(&self, cancel_token: &dyn CancelToken) -> Result<(), TaskError> {
        // 前置检查：启动前先判断是否已取消
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        let args = self.inner.build_args();
        let output = self.command_runner.run_and_capture("ffmpeg", &args)?;

        // 执行后检查取消（虽然中途不能取消，但结束后统一判断语义）
        if cancel_token.is_cancelled() {
            return Err(TaskError::Cancelled);
        }

        // 调用业务任务的输出处理钩子（比如解析 ffprobe JSON）
        self.inner
            .handle_captured_output(&output.stdout, &output.stderr)?;

        if !output.status.success() {
            let err = match output.status.code() {
                Some(code) => anyhow!("FFmpeg exited with non-zero exit code: {code}"),
                None => anyhow!("FFmpeg process terminated by signal/crash, no exit code"),
            };
            return Err(TaskError::Failed(err));
        }

        Ok(())
    }

    /// 启动 `FFmpeg` 子进程，并根据配置启动对应的 `IO` 处理线程
    fn spawn_with_io_handlers(
        &self,
        event_bus: &Arc<dyn EventBus>,
        total_duration: Duration,
        start_time: Instant,
    ) -> Result<(ChildGuard, IoThreadHandles)> {
        let args = self.inner.build_args();
        let command_streams = self.command_runner.spawn("ffmpeg", &args)?;
        let child_guard = ChildGuard::new(command_streams.child_handle);

        let stderr_handle = thread::spawn(move || Self::read_stderr(command_streams.stderr));

        let id = self.id();
        let stdout_handle = if self.inner.needs_progress() {
            let event_bus_clone = event_bus.clone();
            thread::spawn(move || {
                Self::read_progress(
                    id,
                    event_bus_clone.as_ref(),
                    command_streams.stdout,
                    start_time,
                    total_duration,
                )
            })
        } else {
            thread::spawn(move || Self::drain_stdout(command_streams.stdout))
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
    ) -> Result<ExitStatus> {
        // 配置轮询间隔策略
        const INITIAL_SLEEP: Duration = Duration::from_millis(10);
        const MAX_SLEEP: Duration = Duration::from_millis(100);
        const QUICK_CHECKS: u32 = 4;

        let mut sleep_duration = INITIAL_SLEEP;
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
                sleep_duration = (sleep_duration * 2).min(MAX_SLEEP);
            }
        }
    }

    /// 等待 `IO` 线程结束，收集错误，区分取消与业务失败
    fn finalize_streaming_result(
        &self,
        exit_status: ExitStatus,
        io_handles: IoThreadHandles,
        cancel_token: &dyn CancelToken,
    ) -> Result<(), TaskError> {
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
        errors.extend(stdout_res.err());
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
            Ok(())
        } else {
            let summary = format!("Task {} failed with {} errors", self.id(), errors.len());
            Err(TaskError::Failed(join_errors_with_summary(
                summary, &errors,
            )))
        }
    }

    /// 解析 `stdout` 进度并发布事件（有进度任务用）
    fn read_progress(
        id: usize,
        event_bus: &dyn EventBus,
        stdout_reader: impl Read,
        start_time: Instant,
        total_duration: Duration,
    ) -> Result<()> {
        read_progress_impl(id, event_bus, stdout_reader, start_time, total_duration)
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

pub fn read_progress_impl(
    id: usize,
    event_bus: &dyn EventBus,
    stdout_reader: impl Read,
    start_time: Instant,
    total_duration: Duration,
) -> Result<()> {
    let mut buf_reader = BufReader::new(stdout_reader);
    let mut parser = FfmpegProgressParser::default();
    let mut tracker = ProgressTracker::new(total_duration);
    let mut last_publish = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .unwrap_or(Instant::now());
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = buf_reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim_end_matches(['\n', '\r']);
        let Some(raw_progress) = parser.feed_line(trimmed)? else {
            continue;
        };

        let now = Instant::now();
        let elapsed = now - start_time;
        let time_since_last_publish = now - last_publish;
        if time_since_last_publish >= Duration::from_millis(100)
            && let Some(progress) = tracker.update(raw_progress, elapsed)
        {
            event_bus.publish(Event::TaskProgress { id, progress })?;
            last_publish = now;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        infra::{
            MockCancelToken, MockCommandRunner, MockEventBus, MockFileSystem, exit_status,
            exit_status_terminated, exit_status_with_code,
        },
        media_metadata::{MediaMetadata, MockMetadataFetcher, sample_ffprobe_raw_json_bytes},
        tasks::MediaMetadataGetter,
        tasks::ThumbnailGenerator,
    };
    use insta::{assert_debug_snapshot, assert_snapshot};
    use std::path::Path;

    struct StreamingTestSuite {
        wrapper: FfmpegTaskWrapper<ThumbnailGenerator>,
        runner: Arc<MockCommandRunner>,
        fs: Arc<MockFileSystem>,
        fetcher: Arc<MockMetadataFetcher>,
        bus: Arc<dyn EventBus>,
        // 保留具体类型句柄，用于断言事件
        bus_mock: Arc<MockEventBus>,
        cancel: MockCancelToken,
    }

    struct CapturingTestSuite {
        wrapper: FfmpegTaskWrapper<MediaMetadataGetter>,
        runner: Arc<MockCommandRunner>,
        bus: Arc<dyn EventBus>,
        bus_mock: Arc<MockEventBus>,
        cancel: MockCancelToken,
    }

    impl StreamingTestSuite {
        fn new() -> Self {
            let task =
                ThumbnailGenerator::new(1, "/input/test.mp4", Some(Path::new("/output")), 5, None)
                    .unwrap();
            let runner = Arc::new(MockCommandRunner::default());
            let fetcher = Arc::new(MockMetadataFetcher::default());
            let fs = Arc::new(MockFileSystem::default());
            let bus_mock = Arc::new(MockEventBus::default());
            let bus: Arc<dyn EventBus> = bus_mock.clone();
            let cancel = MockCancelToken::default();
            let wrapper = FfmpegTaskWrapper::new(task, runner.clone(), fetcher.clone(), fs.clone());

            Self {
                wrapper,
                runner,
                fs,
                fetcher,
                bus,
                bus_mock,
                cancel,
            }
        }
    }

    impl CapturingTestSuite {
        fn new() -> Self {
            let bus_mock = Arc::new(MockEventBus::default());
            let bus: Arc<dyn EventBus> = bus_mock.clone();
            let task = MediaMetadataGetter::new(1, "/input/test.mp4".into(), bus.clone());
            let runner = Arc::new(MockCommandRunner::default());
            let fetcher = Arc::new(MockMetadataFetcher::default());
            let fs = Arc::new(MockFileSystem::default());
            let cancel = MockCancelToken::default();
            let wrapper = FfmpegTaskWrapper::new(task, runner.clone(), fetcher, fs);

            Self {
                wrapper,
                runner,
                bus,
                bus_mock,
                cancel,
            }
        }
    }

    mod capturing_mode {
        use super::*;

        #[test]
        fn pre_cancelled_aborts_before_execution() {
            let suite = CapturingTestSuite::new();
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err,@"Task cancelled by user");
        }

        #[test]
        fn pre_cancel_in_streaming_aborts_early() {
            let suite = StreamingTestSuite::new();
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
        }

        #[test]
        fn non_zero_exit_code_returns_error() {
            let suite = CapturingTestSuite::new();
            suite.runner.set_capture_ok(
                exit_status_with_code(2),
                sample_ffprobe_raw_json_bytes(),
                vec![],
            );
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Failed(
                "FFmpeg exited with non-zero exit code: 2",
            )
            "#);
        }

        #[test]
        fn signal_terminated_returns_specific_error() {
            let suite = CapturingTestSuite::new();
            suite.runner.set_capture_ok(
                exit_status_terminated(),
                sample_ffprobe_raw_json_bytes(),
                vec![],
            );
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Failed(
                "FFmpeg exited with non-zero exit code: -1",
            )
            "#);
        }

        #[test]
        fn post_cancelled_returns_cancellation_error() {
            let suite = CapturingTestSuite::new();
            suite
                .runner
                .set_capture_ok(exit_status(true), vec![], vec![]);
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err,@"Task cancelled by user");
        }
    }

    mod streaming_mode {
        use super::*;

        #[test]
        fn clean_exit_without_progress_returns_ok() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            assert!(suite.wrapper.run(&suite.bus, &suite.cancel).is_ok());
        }

        #[test]
        fn creates_output_directory_before_spawn() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            let _ = suite.wrapper.run(&suite.bus, &suite.cancel);
            let created = suite.fs.created_dirs.lock().unwrap();
            assert_eq!(created.len(), 1);
            assert_eq!(created[0], Path::new("/output"));
        }

        #[test]
        fn stderr_content_appended_on_exit_failure() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite
                .runner
                .set_spawn_ok(vec![], b"Invalid argument".to_vec(), exit_status(false));
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
            Failed(
                "Task 1 failed with 1 errors\n- Error {\n    context: \"stderr:\\nInvalid argument\",\n    source: \"FFmpeg exited with non-zero exit code: 1\",\n}",
            )
            "#);
        }

        #[test]
        fn failure_without_stderr() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite
                .runner
                .set_spawn_ok(vec![], vec![], exit_status(false));
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
            Failed(
                "Task 1 failed with 1 errors\n- \"FFmpeg exited with non-zero exit code: 1\"",
            )
            "#);
        }

        #[test]
        fn success_ignores_stderr() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite
                .runner
                .set_spawn_ok(vec![], b"just a log".to_vec(), exit_status(true));
            let result = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert!(result.is_ok());
        }

        #[test]
        fn non_zero_exit_aggregates_in_error() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite
                .runner
                .set_spawn_ok(vec![], vec![], exit_status_with_code(3));
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Failed(
                "Task 1 failed with 1 errors\n- \"FFmpeg exited with non-zero exit code: 3\"",
            )
            "#);
        }

        #[test]
        fn mid_execution_cancel_kills_process() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            suite.runner.set_spawn_poll_count(100);
            // 在第二次 is_cancelled 检查时自动触发取消（因为此时子进程已启动）
            suite.cancel.cancel_after(2);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
            let child_inner = suite.runner.last_child_inner().unwrap();
            assert!(child_inner.lock().unwrap().kill_called());
        }

        #[test]
        fn metadata_fetch_failure_aborts_early() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_err("Probe failed");
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Failed(
                "Probe failed",
            )
            "#);
        }

        #[test]
        fn progress_lines_publish_task_progress_events() {
            let suite = StreamingTestSuite::new();
            let mut metadata = MediaMetadata::default();
            metadata.format.duration = Duration::from_secs(10);
            suite.fetcher.set_ok(metadata);
            let stdout = b"out_time_ms=5000000\nspeed=1.8x\nprogress=continue\n\n".to_vec();
            suite.runner.set_spawn_ok(stdout, vec![], exit_status(true));
            let _ = suite.wrapper.run(&suite.bus, &suite.cancel);
            let has_progress = suite
                .bus_mock
                .events()
                .iter()
                .any(|e| matches!(e, Event::TaskProgress { .. }));
            assert!(has_progress);
        }

        #[test]
        fn spawn_failure_propagates_error_early() {
            let suite = StreamingTestSuite::new();
            suite.fetcher.set_ok(MediaMetadata::default());
            suite.runner.set_spawn_err("ffmpeg executable not found");
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Failed(
                "ffmpeg executable not found",
            )
            "#);
        }

        #[test]
        fn progress_events_are_throttled() {
            let suite = StreamingTestSuite::new();
            let mut metadata = MediaMetadata::default();
            metadata.format.duration = Duration::from_secs(10);
            suite.fetcher.set_ok(metadata);
            // 构造两个连续的进度块，时间差极小，远小于 100ms 节流窗口
            let stdout = b"out_time_ms=1000000\nspeed=1.0x\nprogress=continue\n\n\
                            out_time_ms=2000000\nspeed=1.0x\nprogress=continue\n\n"
                .to_vec();
            suite.runner.set_spawn_ok(stdout, vec![], exit_status(true));
            let _ = suite.wrapper.run(&suite.bus, &suite.cancel);
            let progress_events: Vec<_> = suite
                .bus_mock
                .events()
                .into_iter()
                .filter(|e| matches!(e, Event::TaskProgress { .. }))
                .collect();
            assert_eq!(
                progress_events.len(),
                1,
                "Expected exactly one progress event due to throttle, got {}",
                progress_events.len()
            );
        }
    }

    #[test]
    fn wrapper_delegates_id_and_name() {
        let suite = StreamingTestSuite::new();
        assert_eq!(suite.wrapper.id(), 1);
        assert_eq!(suite.wrapper.name(), Some("test"));
    }
}
