use anyhow::{Context, Result, anyhow};
use std::{
    ffi::OsStr,
    fmt,
    io::{self, Read},
    process::{Child, Command, ExitStatus, Stdio},
};

pub trait ChildHandle: Send + Sync + 'static {
    fn wait(&mut self) -> io::Result<ExitStatus>;
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
    fn kill(&mut self) -> io::Result<()>;
}

impl ChildHandle for Child {
    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.wait()
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.try_wait()
    }

    fn kill(&mut self) -> io::Result<()> {
        self.kill()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: ExitStatus,
}

impl CommandOutput {
    pub fn new(stdout: Vec<u8>, stderr: Vec<u8>, status: ExitStatus) -> Self {
        Self {
            stdout,
            stderr,
            status,
        }
    }
}

// 子进程 RAII 守卫：保证 drop 时一定会 kill + wait，彻底避免僵尸进程
pub struct ChildGuard {
    child: Option<Box<dyn ChildHandle>>,
}

impl ChildGuard {
    pub fn new(child: Box<dyn ChildHandle>) -> Self {
        Self { child: Some(child) }
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        let child = self
            .child
            .as_mut()
            .with_context(|| "Failed to get child".to_owned())?;
        child.try_wait().map_err(|e| anyhow!(e))
    }

    pub fn kill(&mut self) -> Result<()> {
        let child = self
            .child
            .as_mut()
            .with_context(|| "Failed to get child".to_owned())?;
        child.kill().map_err(|e| anyhow!(e))
    }

    pub fn wait(&mut self) -> Result<ExitStatus> {
        let mut child = self
            .child
            .take()
            .with_context(|| "Failed to get child".to_owned())?;
        child.wait().map_err(|e| anyhow!(e))
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct CommandStreams {
    pub stdout: Box<dyn Read + Send>,
    pub stderr: Box<dyn Read + Send>,
    pub child_handle: Box<dyn ChildHandle>,
}

impl CommandStreams {
    pub fn new(
        stdout: Box<dyn Read + Send>,
        stderr: Box<dyn Read + Send>,
        child_handle: Box<dyn ChildHandle>,
    ) -> Self {
        Self {
            stdout,
            stderr,
            child_handle,
        }
    }
}

impl fmt::Debug for CommandStreams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandStreams")
            .field("stdout", &"<opaque Read stream>")
            .field("stderr", &"<opaque Read stream>")
            .field("child_handle", &"<opaque ChildHandle>")
            .finish()
    }
}

pub trait StreamingCommandRunner: Send + Sync + 'static {
    fn spawn_raw(&self, program: &OsStr, args: &[&OsStr]) -> Result<CommandStreams>;
}

pub trait StreamingCommandRunnerExt: StreamingCommandRunner {
    /// 友好调用方法：支持 &str / String / Vec<String> 等多种入参
    fn spawn<'a, P, I, S>(&self, program: &'a P, args: I) -> Result<CommandStreams>
    where
        P: AsRef<OsStr> + ?Sized + 'a,
        I: IntoIterator<Item = &'a S>,
        S: AsRef<OsStr> + ?Sized + 'a,
    {
        let program = program.as_ref();
        let args: Vec<&OsStr> = args.into_iter().map(S::as_ref).collect();
        self.spawn_raw(program, &args)
    }
}

// blanket 实现：加 ?Sized，让 dyn 类型也能获得扩展方法
impl<T: StreamingCommandRunner + ?Sized> StreamingCommandRunnerExt for T {}

pub trait CapturingCommandRunner: StreamingCommandRunner {
    /// 默认实现串行读取数据，仅适用于输出量小的场景（如单元测试 Mock），生产环境请覆盖此方法，使用官方 `output()` 避免管道死锁
    fn run_and_capture_raw(&self, program: &OsStr, args: &[&OsStr]) -> Result<CommandOutput> {
        let mut command_streams = self.spawn(program, args)?;

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        command_streams.stdout.read_to_end(&mut stdout_buf)?;
        command_streams.stderr.read_to_end(&mut stderr_buf)?;

        let status = command_streams.child_handle.wait()?;

        Ok(CommandOutput::new(stdout_buf, stderr_buf, status))
    }
}

pub trait CapturingCommandRunnerExt: CapturingCommandRunner {
    /// 友好调用方法：支持 &str / String / Vec<String> 等多种入参，自动完成类型转换
    fn run_and_capture<'a, P, I, S>(&self, program: &'a P, args: I) -> Result<CommandOutput>
    where
        P: AsRef<OsStr> + ?Sized + 'a,
        I: IntoIterator<Item = &'a S>,
        S: AsRef<OsStr> + ?Sized + 'a,
    {
        let program = program.as_ref();
        let args: Vec<&OsStr> = args.into_iter().map(S::as_ref).collect();
        self.run_and_capture_raw(program, &args)
    }
}

// blanket 实现：加 ?Sized，让 dyn 类型也能自动获得扩展方法
impl<T: CapturingCommandRunner + ?Sized> CapturingCommandRunnerExt for T {}

#[derive(Debug, Clone, Default)]
pub struct DefaultCommandRunner;

impl DefaultCommandRunner {
    fn build_command(
        program: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Command {
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::null());
        cmd
    }
}

impl StreamingCommandRunner for DefaultCommandRunner {
    fn spawn_raw(&self, program: &OsStr, args: &[&OsStr]) -> Result<CommandStreams> {
        let mut child = Self::build_command(program, args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn {} command", program.display()))?;

        let stdout = child
            .stdout
            .take()
            .with_context(|| format!("Failed to capture {} stdout", program.display()))?;

        let stderr = child
            .stderr
            .take()
            .with_context(|| format!("Failed to capture {} stderr", program.display()))?;

        Ok(CommandStreams::new(
            Box::new(stdout),
            Box::new(stderr),
            Box::new(child),
        ))
    }
}

impl CapturingCommandRunner for DefaultCommandRunner {
    fn run_and_capture_raw(&self, program: &OsStr, args: &[&OsStr]) -> Result<CommandOutput> {
        let mut cmd = Self::build_command(program, args);

        let output = cmd
            .output()
            .with_context(|| format!("Failed to execute {} command", program.display()))?;

        Ok(CommandOutput::new(
            output.stdout,
            output.stderr,
            output.status,
        ))
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use std::{
        ffi::OsString,
        io::Cursor,
        sync::{Arc, Mutex},
    };

    /// 模拟子进程句柄，可配置轮次、可观测 kill 调用
    #[derive(Debug)]
    pub struct MockChildHandle {
        exit_status: ExitStatus,
        inner: Arc<Mutex<MockChildInner>>,
    }

    #[derive(Debug, Default)]
    pub struct MockChildInner {
        kill_called: bool,
        remaining_polls: u32,
    }

    impl MockChildInner {
        pub fn kill_called(&self) -> bool {
            self.kill_called
        }
    }

    impl MockChildHandle {
        pub fn new(exit_status: ExitStatus, polls_before_complete: u32) -> Self {
            Self {
                exit_status,
                inner: Arc::new(Mutex::new(MockChildInner {
                    kill_called: false,
                    remaining_polls: polls_before_complete,
                })),
            }
        }

        pub fn observer(&self) -> Arc<Mutex<MockChildInner>> {
            self.inner.clone()
        }
    }

    impl ChildHandle for MockChildHandle {
        fn wait(&mut self) -> std::io::Result<ExitStatus> {
            Ok(self.exit_status)
        }

        fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
            let mut inner = self.inner.lock().unwrap();
            if inner.remaining_polls == 0 {
                Ok(Some(self.exit_status))
            } else {
                inner.remaining_polls -= 1;
                Ok(None)
            }
        }

        fn kill(&mut self) -> std::io::Result<()> {
            let mut inner = self.inner.lock().unwrap();
            inner.kill_called = true;
            inner.remaining_polls = 0;
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct MockCommandRunner {
        state: Mutex<RunnerState>,
    }

    #[derive(Default)]
    struct RunnerState {
        // Capture 相关
        capture_ok: Option<CommandOutput>,
        capture_err: Option<String>,
        capture_args_history: Vec<Vec<OsString>>,

        // Spawn 相关
        spawn_stdout: Option<Vec<u8>>,
        spawn_stderr: Option<Vec<u8>>,
        spawn_exit_status: Option<ExitStatus>,
        spawn_poll_count: u32,
        spawn_err: Option<String>,
        spawn_args_history: Vec<Vec<OsString>>,

        // 子进程观察
        last_child_inner: Option<Arc<Mutex<MockChildInner>>>,
    }

    impl MockCommandRunner {
        pub fn set_capture_ok(&self, status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) {
            let mut s = self.state.lock().unwrap();
            s.capture_ok = Some(CommandOutput {
                stdout,
                stderr,
                status,
            });
            s.capture_err = None;
        }

        pub fn set_capture_err(&self, msg: &'static str) {
            let mut s = self.state.lock().unwrap();
            s.capture_err = Some(msg.to_string());
            s.capture_ok = None;
        }

        pub fn set_spawn_ok(&self, stdout: Vec<u8>, stderr: Vec<u8>, exit_status: ExitStatus) {
            let mut s = self.state.lock().unwrap();
            s.spawn_stdout = Some(stdout);
            s.spawn_stderr = Some(stderr);
            s.spawn_exit_status = Some(exit_status);
            s.spawn_poll_count = 0;
            s.spawn_err = None;
        }

        pub fn set_spawn_poll_count(&self, polls: u32) {
            self.state.lock().unwrap().spawn_poll_count = polls;
        }

        /// spawn 错误只消费一次（内部 take）
        pub fn set_spawn_err(&self, msg: &'static str) {
            let mut s = self.state.lock().unwrap();
            s.spawn_err = Some(msg.to_string());
        }

        pub fn last_spawn_args(&self) -> Vec<OsString> {
            self.state
                .lock()
                .unwrap()
                .spawn_args_history
                .last()
                .cloned()
                .unwrap_or_default()
        }

        pub fn spawn_call_count(&self) -> usize {
            self.state.lock().unwrap().spawn_args_history.len()
        }

        pub fn last_capture_args(&self) -> Vec<OsString> {
            self.state
                .lock()
                .unwrap()
                .capture_args_history
                .last()
                .cloned()
                .unwrap_or_default()
        }

        pub fn capture_call_count(&self) -> usize {
            self.state.lock().unwrap().capture_args_history.len()
        }

        pub fn last_child_inner(&self) -> Option<Arc<Mutex<MockChildInner>>> {
            self.state.lock().unwrap().last_child_inner.clone()
        }
    }

    impl StreamingCommandRunner for MockCommandRunner {
        fn spawn_raw(&self, _program: &OsStr, args: &[&OsStr]) -> Result<CommandStreams> {
            let mut s = self.state.lock().unwrap();
            s.spawn_args_history
                .push(args.iter().map(|a| a.to_os_string()).collect());

            // 启动错误一次性消费
            if let Some(err) = s.spawn_err.take() {
                return Err(anyhow!(err));
            }

            let stdout = s.spawn_stdout.clone().unwrap_or_default();
            let stderr = s.spawn_stderr.clone().unwrap_or_default();
            let exit_status = s
                .spawn_exit_status
                .ok_or_else(|| anyhow!("No spawn exit status configured"))?;
            let poll_count = s.spawn_poll_count;

            let child = MockChildHandle::new(exit_status, poll_count);
            s.last_child_inner = Some(child.observer());

            Ok(CommandStreams::new(
                Box::new(Cursor::new(stdout)),
                Box::new(Cursor::new(stderr)),
                Box::new(child),
            ))
        }
    }

    impl CapturingCommandRunner for MockCommandRunner {
        fn run_and_capture_raw(&self, _program: &OsStr, args: &[&OsStr]) -> Result<CommandOutput> {
            let mut s = self.state.lock().unwrap();
            s.capture_args_history
                .push(args.iter().map(|a| a.to_os_string()).collect());

            if let Some(msg) = &s.capture_err {
                return Err(anyhow!(msg.clone()));
            }

            s.capture_ok
                .clone()
                .ok_or_else(|| anyhow!("No capture result set"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::test_utils::{
        MockChildHandle, MockCommandRunner, exit_status, exit_status_with_code,
    };
    use insta::assert_debug_snapshot;

    #[test]
    fn child_guard_try_wait_returns_none_before_complete() {
        let child = MockChildHandle::new(exit_status(true), 2);
        let mut guard = ChildGuard::new(Box::new(child));
        assert!(guard.try_wait().unwrap().is_none());
        assert!(guard.try_wait().unwrap().is_none());
        assert!(guard.try_wait().unwrap().is_some());
    }

    #[test]
    fn child_guard_wait_consumes_child() {
        let child = MockChildHandle::new(exit_status(true), 0);
        let mut guard = ChildGuard::new(Box::new(child));
        let status = guard.wait().unwrap();
        assert!(status.success());
        // 二次 wait 应报错（child 已被 take）
        assert!(guard.wait().is_err());
    }

    #[test]
    fn child_guard_kill_sets_flag() {
        let child = MockChildHandle::new(exit_status(false), 10);
        let observer = child.observer();
        let mut guard = ChildGuard::new(Box::new(child));
        guard.kill().unwrap();
        assert!(observer.lock().unwrap().kill_called());
    }

    #[test]
    fn child_guard_drop_auto_kills_and_waits() {
        let child = MockChildHandle::new(exit_status(false), 10);
        let observer = child.observer();
        {
            let _guard = ChildGuard::new(Box::new(child));
            // 作用域结束自动 drop
        }
        assert!(observer.lock().unwrap().kill_called());
    }

    #[test]
    fn streaming_ext_supports_mixed_arg_types() {
        let runner = MockCommandRunner::default();
        runner.set_spawn_ok(vec![], vec![], exit_status(true));
        // 支持 &str 数组、String 等多种入参
        let args = vec!["-i", "input.mp4"];
        let result = runner.spawn("ffmpeg", args);
        assert!(result.is_ok());
    }

    #[test]
    fn capturing_ext_supports_mixed_arg_types() {
        let runner = MockCommandRunner::default();
        runner.set_capture_ok(exit_status(true), vec![], vec![]);
        let result = runner.run_and_capture("ffmpeg", ["-version"]);
        assert!(result.is_ok());
    }

    #[test]
    fn mock_runner_capture_returns_configured_result() {
        let runner = MockCommandRunner::default();
        runner.set_capture_ok(
            exit_status_with_code(1),
            b"stdout".to_vec(),
            b"stderr".to_vec(),
        );
        let output = runner.run_and_capture("test", &["arg"]).unwrap();
        assert!(!output.status.success());
        assert_debug_snapshot!(output.stdout,@"
        [
            115,
            116,
            100,
            111,
            117,
            116,
        ]
        ");
        assert_debug_snapshot!(output.stderr,@"
        [
            115,
            116,
            100,
            101,
            114,
            114,
        ]
        ");
    }

    #[test]
    fn mock_runner_capture_no_config_returns_error() {
        let runner = MockCommandRunner::default();
        let err = runner.run_and_capture("test", &["arg"]).unwrap_err();
        assert_debug_snapshot!(err,@r#""No capture result set""#);
    }

    #[test]
    #[cfg(unix)]
    fn default_runner_capture_true_returns_success() {
        let runner = DefaultCommandRunner::default();
        let output = runner.run_and_capture("true", &[] as &[&str]).unwrap();
        assert!(output.status.success());
    }

    #[test]
    #[cfg(unix)]
    fn default_runner_capture_false_returns_failure() {
        let runner = DefaultCommandRunner::default();
        let output = runner.run_and_capture("false", &[] as &[&str]).unwrap();
        assert!(!output.status.success());
    }

    #[test]
    #[cfg(windows)]
    fn default_runner_capture_exit_zero_returns_success() {
        let runner = DefaultCommandRunner;
        let output = runner.run_and_capture("cmd", &["/c", "exit 0"]).unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn default_runner_spawn_reads_stdout() {
        let runner = DefaultCommandRunner;
        #[cfg(unix)]
        let (cmd, args) = ("echo", &["hello"] as &[&str]);
        #[cfg(windows)]
        let (cmd, args) = ("cmd", &["/c", "echo hello"]);
        let mut streams = runner.spawn(cmd, args).unwrap();
        let mut buf = String::new();
        streams.stdout.read_to_string(&mut buf).unwrap();
        let expected = if cfg!(windows) {
            "hello\r\n"
        } else {
            "hello\n"
        };
        assert_eq!(buf, expected);
    }

    #[test]
    fn mock_runner_spawn_err_returns_configured_error() {
        let runner = MockCommandRunner::default();
        runner.set_spawn_err("command not found");
        let (cmd, args): (&str, &[&str; 0]) = ("nonexistent_cmd", &[]);
        let err = runner.spawn(cmd, args).unwrap_err();
        assert_debug_snapshot!(err,@r#""command not found""#);
    }

    #[test]
    fn mock_runner_spawn_err_is_consumed_once() {
        let runner = MockCommandRunner::default();
        runner.set_spawn_err("spawn failed");
        let (cmd, args): (&str, &[&str; 0]) = ("nonexistent_cmd", &[]);
        assert!(runner.spawn(cmd, args).is_err());
    }
}
