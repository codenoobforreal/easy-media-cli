mod default_executor;

use crate::progress::{RawProgress, RawProgressEvent};
use anyhow::{Result, anyhow};
use std::{
    process::{Child, Command},
    sync::mpsc::Receiver,
    thread::JoinHandle,
};

pub use default_executor::DefaultCommandExecutor;

pub trait CommandExecutor: Send {
    fn execute(&self, cmd: &mut Command) -> Result<String>;

    fn execute_streaming(
        &self,
        cmd: &mut Command,
    ) -> Result<Box<dyn Iterator<Item = Result<RawProgressEvent>> + Send>>;

    fn execute_with_progress(
        &self,
        cmd: &mut Command,
        progress_cb: &mut (dyn FnMut(RawProgress) + Send),
    ) -> Result<()> {
        let events = self.execute_streaming(cmd)?;
        let mut stderr_buffer = String::new();

        for event in events {
            match event? {
                RawProgressEvent::Stdout(progress) => progress_cb(progress),
                RawProgressEvent::Stderr(msg) => {
                    stderr_buffer.push_str(&msg);
                    stderr_buffer.push('\n');
                }
                RawProgressEvent::Error(err) => return Err(anyhow!(err)),
            }
        }

        Ok(())
    }
}

pub struct ProgressStreamingIterator {
    receiver: Receiver<RawProgressEvent>,
    child: Option<Child>,
    stdout_handle: Option<JoinHandle<Result<()>>>,
    stderr_handle: Option<JoinHandle<Result<()>>>,
    finished: bool,
}

impl Iterator for ProgressStreamingIterator {
    type Item = Result<RawProgressEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        match self.receiver.recv() {
            Ok(event) => Some(Ok(event)),
            Err(_) => {
                self.finished = true;
                self.finalize()
            }
        }
    }
}

impl ProgressStreamingIterator {
    pub fn new(
        receiver: Receiver<RawProgressEvent>,
        child: Option<Child>,
        stdout_handle: Option<JoinHandle<Result<()>>>,
        stderr_handle: Option<JoinHandle<Result<()>>>,
        finished: bool,
    ) -> Self {
        Self {
            receiver,
            child,
            stdout_handle,
            stderr_handle,
            finished,
        }
    }

    fn finalize(&mut self) -> Option<Result<RawProgressEvent>> {
        if let Some(handle) = self.stdout_handle.take() {
            if let Err(e) = handle.join().map_err(|_| anyhow!("Stdout thread panicked")) {
                return Some(Err(e));
            }
        }

        if let Some(handle) = self.stderr_handle.take() {
            if let Err(e) = handle.join().map_err(|_| anyhow!("Stderr thread panicked")) {
                return Some(Err(e));
            }
        }

        let mut child = self.child.take()?;
        let status = match child.wait() {
            Ok(s) => s,
            Err(e) => return Some(Err(anyhow!("Failed to wait for child: {}", e))),
        };

        if !status.success() {
            Some(Err(anyhow!(
                "Command failed with code {}",
                status.code().unwrap_or(-1)
            )))
        } else {
            None
        }
    }
}

impl Drop for ProgressStreamingIterator {
    fn drop(&mut self) {
        if let Some(handle) = self.stdout_handle.take() {
            let _ = handle.join();
        }

        if let Some(handle) = self.stderr_handle.take() {
            let _ = handle.join();
        }

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
