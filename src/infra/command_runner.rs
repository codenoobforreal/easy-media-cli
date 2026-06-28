use anyhow::{Context, Result, anyhow};
use std::{
    ffi::{OsStr, OsString},
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

#[derive(Debug, Clone, Default)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>, args: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            args,
        }
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

pub trait CapturingCommandRunner: StreamingCommandRunner + fmt::Debug {
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
