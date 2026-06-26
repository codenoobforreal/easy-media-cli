//! `FfmpegTaskWrapper` + 通用执行流程

use crate::{
    common::join_errors_with_summary,
    domain::{CancelToken, Event, Fetcher as MetadataFetcher, Task, TaskError},
    infra::{
        CapturingCommandRunner, CapturingCommandRunnerExt, ChildGuard, EventBus,
        FfmpegProgressParser, FileSystem, ProgressTracker, StreamingCommandRunnerExt,
    },
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
    render_interval: Duration,
    progress_threshold: f32,
}

impl<T: FfmpegTask> Task for FfmpegTaskWrapper<T> {
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
        render_interval: Duration,
        progress_threshold: f32,
    ) -> Self {
        Self {
            inner,
            command_runner,
            metadata_fetcher,
            file_system,
            render_interval,
            progress_threshold,
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

        let total_duration = if let Some(d) = self.inner.duration() {
            d
        } else {
            self.metadata_fetcher
                .fetch_metadata(self.inner.input())?
                .duration()
        };
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
        render_interval: Duration,
        progress_threshold: f32,
    ) -> Result<(ChildGuard, IoThreadHandles)> {
        let args = self.inner.build_args();
        let command_streams = self.command_runner.spawn("ffmpeg", &args)?;
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
) -> Result<()> {
    let mut buf_reader = BufReader::new(stdout_reader);
    let mut parser = FfmpegProgressParser::default();
    let mut tracker = ProgressTracker::new(total_duration, progress_threshold);
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
        if time_since_last_publish >= render_interval
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
        cli::GlobalConfig,
        domain::Metadata as MediaMetadata,
        infra::{
            MockFileSystem,
            test_utils::{
                MockCancelToken, MockCommandRunner, MockEventBus, MockMetadataFetcher, exit_status,
                exit_status_terminated, exit_status_with_code,
            },
        },
    };
    use insta::{assert_debug_snapshot, assert_snapshot};
    use std::{
        ffi::{OsStr, OsString},
        path::{Path, PathBuf},
    };

    struct TestSuite<T: FfmpegTask> {
        wrapper: FfmpegTaskWrapper<T>,
        runner: Arc<MockCommandRunner>,
        fs: Arc<MockFileSystem>,
        fetcher: Arc<MockMetadataFetcher>,
        bus: Arc<dyn EventBus>,
        bus_mock: Arc<MockEventBus>,
        cancel: MockCancelToken,
    }

    impl<T: FfmpegTask> TestSuite<T> {
        fn new(task: T) -> Self {
            let config = GlobalConfig::parser_default();
            dbg!(&config);
            Self::with_config(
                task,
                Duration::from_millis(config.render_interval_ms),
                config.progress_threshold,
            )
        }

        fn with_config(task: T, render_interval: Duration, progress_threshold: f32) -> Self {
            let runner = Arc::new(MockCommandRunner::default());
            let fetcher = Arc::new(MockMetadataFetcher::default());
            let fs = Arc::new(MockFileSystem::default());
            let bus_mock = Arc::new(MockEventBus::default());
            let bus: Arc<dyn EventBus> = bus_mock.clone();
            let cancel = MockCancelToken::default();
            let wrapper = FfmpegTaskWrapper::new(
                task,
                runner.clone(),
                fetcher.clone(),
                fs.clone(),
                render_interval,
                progress_threshold,
            );
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

    #[allow(clippy::type_complexity)]
    struct MockFfmpegTask {
        id: usize,
        name: String,
        input: PathBuf,
        output: Option<PathBuf>,
        args: Vec<OsString>,
        needs_progress: bool,
        execution_mode: ExecutionMode,
        duration: Option<Duration>,
        needs_output_dir: bool,
        captured_output_handler: Option<Box<dyn Fn(&[u8], &[u8]) -> Result<()> + Send + Sync>>,
    }

    impl MockFfmpegTask {
        fn new(id: usize) -> Self {
            Self {
                id,
                name: "test".into(),
                input: PathBuf::from("/input/test.mp4"),
                output: None,
                args: vec![],
                needs_progress: true,
                execution_mode: ExecutionMode::Streaming,
                duration: None,
                needs_output_dir: false,
                captured_output_handler: None,
            }
        }

        fn with_name(mut self, name: &str) -> Self {
            self.name = name.into();
            self
        }

        fn with_output(mut self, path: Option<&str>) -> Self {
            self.output = path.map(PathBuf::from);
            self.needs_output_dir = path.is_some();
            self
        }

        #[allow(dead_code)]
        fn with_args(mut self, args: Vec<OsString>) -> Self {
            self.args = args;
            self
        }

        fn with_needs_progress(mut self, needs: bool) -> Self {
            self.needs_progress = needs;
            self
        }

        fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
            self.execution_mode = mode;
            self
        }

        fn with_duration(mut self, d: Option<Duration>) -> Self {
            self.duration = d;
            self
        }

        #[allow(dead_code)]
        fn with_captured_handler(
            mut self,
            handler: impl Fn(&[u8], &[u8]) -> Result<()> + Send + Sync + 'static,
        ) -> Self {
            self.captured_output_handler = Some(Box::new(handler));
            self
        }
    }

    impl FfmpegTask for MockFfmpegTask {
        fn id(&self) -> usize {
            self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input(&self) -> &Path {
            &self.input
        }

        fn output(&self) -> Option<&Path> {
            self.output.as_deref()
        }

        fn build_args(&self) -> Vec<OsString> {
            self.args.clone()
        }

        fn file_name(&self) -> Option<&OsStr> {
            self.input.file_name()
        }

        fn needs_progress(&self) -> bool {
            self.needs_progress
        }

        fn execution_mode(&self) -> ExecutionMode {
            self.execution_mode
        }

        fn duration(&self) -> Option<Duration> {
            self.duration
        }

        fn needs_output_dir(&self) -> bool {
            self.needs_output_dir
        }

        fn handle_captured_output(&self, stdout: &[u8], stderr: &[u8]) -> Result<()> {
            if let Some(ref handler) = self.captured_output_handler {
                handler(stdout, stderr)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn wrapper_delegates_id_and_name() {
        let task = MockFfmpegTask::new(42).with_name("delegated");
        let suite = TestSuite::new(task);
        assert_eq!(suite.wrapper.id(), 42);
        assert_eq!(suite.wrapper.name(), "delegated");
    }

    mod streaming {
        use super::*;

        /// 创建一个典型的 streaming 任务，提供时长以跳过 fetcher。
        fn basic_task() -> MockFfmpegTask {
            MockFfmpegTask::new(1)
                .with_execution_mode(ExecutionMode::Streaming)
                .with_duration(Some(Duration::from_secs(10)))
        }

        #[test]
        fn pre_cancelled_aborts_early() {
            let task = basic_task();
            let suite = TestSuite::new(task);
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
            // 未触发任何 spawn
            assert_eq!(suite.runner.spawn_call_count(), 0);
        }

        #[test]
        fn clean_exit_without_progress_returns_ok() {
            let task = basic_task().with_needs_progress(false);
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            assert!(suite.wrapper.run(&suite.bus, &suite.cancel).is_ok());
            assert_eq!(suite.fetcher.call_count(), 0);
        }

        #[test]
        fn creates_output_directory_before_spawn() {
            let task = basic_task().with_output(Some("/output/test.mp4"));
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            let _ = suite.wrapper.run(&suite.bus, &suite.cancel);
            let dirs = suite.fs.created_dirs();
            assert_eq!(dirs.len(), 1);
            assert_eq!(dirs[0], Path::new("/output"));
        }

        #[test]
        fn stderr_content_appended_on_exit_failure() {
            let task = basic_task();
            let suite = TestSuite::new(task);
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
            let task = basic_task();
            let suite = TestSuite::new(task);
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
            let task = basic_task();
            let suite = TestSuite::new(task);
            suite
                .runner
                .set_spawn_ok(vec![], b"just a log".to_vec(), exit_status(true));
            let result = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert!(result.is_ok());
        }

        #[test]
        fn non_zero_exit_aggregates_in_error() {
            let task = basic_task();
            let suite = TestSuite::new(task);
            suite
                .runner
                .set_spawn_ok(vec![], vec![], exit_status_with_code(3));
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "Task 1 failed with 1 errors\n- \"FFmpeg exited with non-zero exit code: 3\"",
              )
              "#);
        }

        #[test]
        fn mid_execution_cancel_kills_process() {
            let task = basic_task();
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            suite.runner.set_spawn_poll_count(100);
            suite.cancel.cancel_after(2);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
            let child_inner = suite.runner.last_child_inner().unwrap();
            assert!(child_inner.lock().unwrap().kill_called());
        }

        #[test]
        fn metadata_fetch_failure_aborts_early() {
            // 该测试特意让 duration 为 None 以触发 fetcher 错误
            let task = MockFfmpegTask::new(1)
                .with_execution_mode(ExecutionMode::Streaming)
                .with_duration(None);
            let suite = TestSuite::new(task);
            suite.fetcher.set_err("Probe failed");
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "Probe failed",
              )
              "#);
            assert_eq!(suite.fetcher.call_count(), 1);
        }

        #[test]
        fn progress_lines_publish_task_progress_events() {
            let task = basic_task(); // duration 10s
            let suite = TestSuite::new(task);
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
            let task = basic_task();
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_err("ffmpeg executable not found");
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "ffmpeg executable not found",
              )
              "#);
        }

        #[test]
        fn progress_events_are_throttled() {
            let task = basic_task(); // duration 10s
            let suite = TestSuite::new(task);
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

    mod capturing {
        use super::*;

        fn basic_task() -> MockFfmpegTask {
            MockFfmpegTask::new(1)
                .with_execution_mode(ExecutionMode::Capturing)
                .with_needs_progress(false)
                .with_duration(None)
        }

        #[test]
        fn pre_cancelled_aborts_before_execution() {
            let suite = TestSuite::new(basic_task());
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
            assert_eq!(suite.runner.capture_call_count(), 0);
        }

        #[test]
        fn non_zero_exit_code_returns_error() {
            let suite = TestSuite::new(basic_task());
            suite
                .runner
                .set_capture_ok(exit_status_with_code(2), b"any stdout".to_vec(), vec![]);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "FFmpeg exited with non-zero exit code: 2",
              )
              "#);
        }

        #[test]
        #[cfg(unix)]
        fn signal_terminated_returns_specific_error_unix() {
            let suite = TestSuite::new(basic_task());
            suite
                .runner
                .set_capture_ok(exit_status_terminated(), b"any stdout".to_vec(), vec![]);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "FFmpeg process terminated by signal/crash, no exit code",
              )
              "#);
        }

        #[test]
        #[cfg(windows)]
        fn signal_terminated_returns_specific_error_windows() {
            let suite = TestSuite::new(basic_task());
            suite
                .runner
                .set_capture_ok(exit_status_terminated(), b"any stdout".to_vec(), vec![]);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_debug_snapshot!(err, @r#"
              Failed(
                  "FFmpeg exited with non-zero exit code: -1",
              )
              "#);
        }

        #[test]
        fn post_cancelled_returns_cancellation_error() {
            let suite = TestSuite::new(basic_task());
            suite
                .runner
                .set_capture_ok(exit_status(true), vec![], vec![]);
            // 在执行前设置取消，使执行后的取消检查触发
            suite.cancel.set_cancelled(true);
            let err = suite.wrapper.run(&suite.bus, &suite.cancel).unwrap_err();
            assert_snapshot!(err, @"Task cancelled by user");
        }

        #[test]
        fn capturing_mode_never_calls_fetcher() {
            let suite = TestSuite::new(basic_task());
            suite
                .runner
                .set_capture_ok(exit_status(true), vec![], vec![]);
            let _ = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert_eq!(suite.fetcher.call_count(), 0);
        }
    }

    mod duration_optimization {
        use super::*;

        fn streaming_task_with_duration(d: Duration) -> MockFfmpegTask {
            MockFfmpegTask::new(1)
                .with_execution_mode(ExecutionMode::Streaming)
                .with_duration(Some(d))
        }

        #[test]
        fn duration_provided_skips_fetch_metadata() {
            let task = streaming_task_with_duration(Duration::from_secs(10));
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            let result = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert!(result.is_ok());
            assert_eq!(suite.fetcher.call_count(), 0);
        }

        #[test]
        fn duration_provided_zero_does_not_panic() {
            let task = streaming_task_with_duration(Duration::ZERO);
            let suite = TestSuite::new(task);
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            let result = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert!(result.is_ok());
        }

        #[test]
        fn streaming_falls_back_to_fetcher_when_no_duration() {
            let task = MockFfmpegTask::new(1)
                .with_execution_mode(ExecutionMode::Streaming)
                .with_duration(None);
            let suite = TestSuite::new(task);
            suite.fetcher.set_ok(MediaMetadata::default());
            suite.runner.set_spawn_ok(vec![], vec![], exit_status(true));
            let result = suite.wrapper.run(&suite.bus, &suite.cancel);
            assert!(result.is_ok());
            assert_eq!(suite.fetcher.call_count(), 1);
        }
    }
}
