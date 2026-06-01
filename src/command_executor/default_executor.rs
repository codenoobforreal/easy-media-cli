use crate::{
    command_executor::{CommandExecutor, ProgressStreamingIterator},
    progress::{RawProgress, RawProgressEvent, parse_raw_progress},
};
use anyhow::{Result, anyhow};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};

#[derive(Debug, Default)]
pub struct DefaultCommandExecutor;

impl CommandExecutor for DefaultCommandExecutor {
    fn execute(&self, cmd: &mut Command) -> Result<String> {
        let output = cmd.output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?)
        } else {
            let stderr = String::from_utf8(output.stderr)?;
            Err(anyhow!(
                "Command {} failed with code {}\n{}",
                cmd.get_program().to_string_lossy(),
                output.status.code().unwrap_or(-1),
                stderr
            ))
        }
    }

    fn execute_streaming(
        &self,
        cmd: &mut Command,
    ) -> Result<Box<dyn Iterator<Item = Result<RawProgressEvent>> + Send>> {
        let mut child = cmd.spawn()?;
        let (receiver, stdout_handle, stderr_handle) = Self::setup_channel(&mut child)?;

        Ok(Box::new(ProgressStreamingIterator::new(
            receiver,
            Some(child),
            Some(stdout_handle),
            Some(stderr_handle),
            false,
        )))
    }

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

        if !stderr_buffer.is_empty() {
            return Err(anyhow!(stderr_buffer));
        }

        Ok(())
    }
}

impl DefaultCommandExecutor {
    fn setup_channel(
        child: &mut Child,
    ) -> Result<(
        Receiver<RawProgressEvent>,
        JoinHandle<Result<()>>,
        JoinHandle<Result<()>>,
    )> {
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Unable to access child process stderr",))?;
        let stderr_reader = BufReader::new(stderr);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Unable to access child process stdout",))?;
        let stdout_reader = BufReader::new(stdout);

        let (raw_progress_sender, raw_progress_receiver) = mpsc::channel::<RawProgressEvent>();

        let stdout_clone_sender = raw_progress_sender.clone();
        let stdout_handle = thread::spawn(move || {
            let progress_iter = parse_raw_progress(stdout_reader.lines());
            for progress in progress_iter {
                match progress {
                    Ok(p) => {
                        let _ = stdout_clone_sender.send(RawProgressEvent::Stdout(p));
                    }
                    Err(e) => {
                        let _ = stdout_clone_sender.send(RawProgressEvent::Error(e.to_string()));
                    }
                }
            }
            Ok(())
        });

        let stderr_clone_sender = raw_progress_sender.clone();
        let stderr_handle = thread::spawn(move || {
            for line in stderr_reader.lines() {
                let line = line?;
                if !line.is_empty() {
                    let _ = stderr_clone_sender.send(RawProgressEvent::Stderr(line));
                }
            }
            Ok(())
        });

        Ok((raw_progress_receiver, stdout_handle, stderr_handle))
    }
}
