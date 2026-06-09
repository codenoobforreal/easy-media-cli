use crate::{
    event::{Event, EventBus},
    metadata::metadata,
    task::{Progress, Task, progress::RawProgress},
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{ChildStderr, ChildStdout, Command},
    select, spawn,
    time::Instant,
};
use tokio_util::sync::CancellationToken;

const FFMPEG_QUIET: &[&str] = &["-v", "error"];
const FFMPEG_PROGRESS: &[&str] = &["-progress", "pipe:1"];
const FFMPEG_OVERWRITE: &[&str] = &["-y"];

#[derive(Debug)]
pub struct Thumbnail {
    id: u64,
    input: PathBuf,
    output: PathBuf,
    scene_threshold: f32,
    width: Option<u16>,
}

impl Thumbnail {
    pub fn new(
        id: u64,
        input: PathBuf,
        output: PathBuf,
        scene_threshold: f32,
        width: Option<u16>,
    ) -> Self {
        let output = Self::build_thumbnail_output(&input, &output);
        Self {
            id,
            input,
            output,
            scene_threshold,
            width,
        }
    }

    fn build_thumbnail_command(&self) -> Command {
        let vf_str = match self.width {
            None => format!(
                "select='gt(scene,{:.1})',scale=in_range=auto:out_range=full,format=yuvj420p",
                self.scene_threshold
            ),
            Some(w) => format!(
                "select='gt(scene,{:.1})',scale=in_range=auto:out_range=full,format=yuvj420p:{w}:-2",
                self.scene_threshold
            ),
        };
        let mut cmd = Command::new("ffmpeg");
        cmd.args(FFMPEG_QUIET);
        cmd.args(["-skip_frame", "nokey"]);
        cmd.args(FFMPEG_PROGRESS);
        cmd.arg("-i");
        cmd.arg(&self.input);
        cmd.args(["-vf", &vf_str]);
        cmd.args(["-fps_mode", "vfr"]);
        cmd.args(["-q:v", "2"]);
        cmd.args(FFMPEG_OVERWRITE);
        cmd.arg(&self.output);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // dbg!(&cmd);
        cmd
    }

    /// When the frame is 0, the other progress fields are typically 'N/A', so parsing is not performed for this round.
    async fn send_progress(
        event_bus: EventBus,
        id: u64,
        stdout: ChildStdout,
        start_time: Instant,
        total_duration: Duration,
    ) -> Result<()> {
        let mut line_reader = BufReader::new(stdout).lines();
        let mut out_time_ms = None;
        let mut speed = None;
        let mut last_progress = Progress::default();
        while let Some(line) = line_reader
            .next_line()
            .await
            .with_context(|| format!("Failed to read progress line"))?
        {
            let trim_line = line.trim();
            if trim_line.is_empty() {
                continue;
            }
            let (key, value) = match trim_line.split_once('=') {
                Some((k, v)) if !v.is_empty() && v != "N/A" => (k, v),
                _ => continue,
            };
            match key {
                "out_time_ms" => {
                    // println!("out_time_ms={value}");
                    out_time_ms = Some(
                        value
                            .parse::<u64>()
                            .with_context(|| format!("Failed to parse u64: {key}={value}"))?,
                    );
                }
                "speed" => {
                    // println!("speed={value}");
                    speed = Some(
                        value
                            .trim()
                            .strip_suffix('x')
                            .with_context(|| format!("Failed to strip suffix x: {key}={value}"))?
                            .parse::<f32>()
                            .with_context(|| format!("Failed to parse f32: {key}={value}"))?,
                    );
                }
                "progress" => {
                    if let (Some(s), Some(otm)) = (speed, out_time_ms) {
                        let raw_progress = RawProgress::new(s, otm);
                        let progress = Progress::from_raw_progress(
                            raw_progress,
                            total_duration,
                            start_time.elapsed(),
                        );
                        if progress.should_update(&last_progress) {
                            // println!("{:?}", progress);
                            event_bus.publish(Event::TaskProgress { id, progress })?;
                            last_progress = progress;
                            out_time_ms = None;
                            speed = None;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn send_error(event_bus: EventBus, id: u64, stderr: ChildStderr) -> Result<()> {
        let mut error_buf = String::new();
        if BufReader::new(stderr)
            .read_to_string(&mut error_buf)
            .await
            .with_context(|| format!("Stderr output is not utf-8"))?
            > 0
        {
            return event_bus.publish(Event::TaskFailed {
                id,
                error: error_buf,
            });
        }
        Ok(())
    }

    fn build_thumbnail_output<P: AsRef<Path>>(input: P, output: P) -> PathBuf {
        let mut out_path = output.as_ref().to_path_buf();
        let stem = input.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
        out_path.push(format!("{stem}-%04d.jpg"));
        // dbg!(&out_path);
        out_path
    }
}

#[async_trait]
impl Task for Thumbnail {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        "thumbnail generate"
    }

    fn file_name(&self) -> Option<&str> {
        self.input.file_name().and_then(|name| name.to_str())
    }

    async fn run(&self, event_bus: EventBus, cancel_token: CancellationToken) -> Result<()> {
        event_bus.publish(Event::TaskStarted { id: self.id })?;
        let metadata = metadata(&self.input).await?;
        let total_duration = metadata.duration();
        let mut child = self
            .build_thumbnail_command()
            .spawn()
            .with_context(|| format!("Failed to spawn thumbnail command"))?;
        let stdout = child
            .stdout
            .take()
            .with_context(|| format!("Failed to capture thumbnail ffmpeg stdout"))?;
        let stderr = child
            .stderr
            .take()
            .with_context(|| format!("Failed to capture thumbnail ffmpeg stdout"))?;
        let start_time = Instant::now();
        let id = self.id;
        let mut stdout_handle = spawn(Self::send_progress(
            event_bus.clone(),
            id,
            stdout,
            start_time,
            total_duration,
        ));
        let mut stderr_handle = spawn(Self::send_error(event_bus.clone(), id, stderr));
        let (mut stdout_done, mut stderr_done) = (false, false);
        let mut stdout_res: Option<Result<()>> = None;
        let mut stderr_res: Option<Result<()>> = None;
        loop {
            select! {
                res = &mut stdout_handle, if !stdout_done => {
                    stdout_done = true;
                    stdout_res = Some(res.with_context(|| "Stdout progress task panicked")?);
                }
                res = &mut stderr_handle, if !stderr_done => {
                    stderr_done = true;
                    stderr_res = Some(res.with_context(|| "Stderr error task panicked")?);
                }
                _ = cancel_token.cancelled() => {
                    let _ = child.kill().await;
                    stdout_handle.abort();
                    stderr_handle.abort();
                    let _ = child.wait().await;
                    // bail!("Task cancelled by user (Ctrl+C)");
                }
            }
            if stdout_done && stderr_done {
                break;
            }
        }
        let child_status = child
            .wait()
            .await
            .with_context(|| "Failed to wait for thumbnail process")?;
        let mut errors = vec![];
        if let Some(Err(e)) = stdout_res {
            errors.push(e);
        }
        if let Some(Err(e)) = stderr_res {
            errors.push(e);
        }
        if !child_status.success() {
            errors.push(anyhow!(
                "FFmpeg exited with non-zero code: {}",
                child_status.code().unwrap_or(-1)
            ));
        }
        if !errors.is_empty() {
            let mut main_err = anyhow!("Thumbnail task {} failed with {} errors", id, errors.len());
            for err in errors {
                main_err = main_err.context(err);
            }
            return Err(main_err);
        }
        event_bus.publish(Event::TaskCompleted { id })?;
        Ok(())
    }
}
