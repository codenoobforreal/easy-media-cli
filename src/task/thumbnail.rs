use crate::{
    error::{AppError, AppResult},
    event::{Event, EventBus},
    metadata::metadata,
    task::{Progress, Task, progress::RawProgress},
};
use async_trait::async_trait;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{ChildStderr, ChildStdout, Command},
    time::Instant,
};

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

        cmd
    }

    /// When the frame is 0, the other progress fields are typically 'N/A', so parsing is not performed for this round.
    async fn send_progress(
        event_bus: EventBus,
        id: u64,
        stdout: ChildStdout,
        start_time: Instant,
        total_duration: Duration,
    ) -> () {
        let mut line_reader = BufReader::new(stdout).lines();
        let mut out_time_ms = None;
        let mut speed = None;
        let mut last_progress = Progress::default();
        while let Some(line) = line_reader.next_line().await.unwrap() {
            let trim_line = line.trim();
            if trim_line.is_empty() {
                continue;
            }
            let (key, value) = match trim_line.split_once('=') {
                Some((k, v)) => (k, v),
                _ => continue,
            };

            match key {
                "out_time_ms" => {
                    if value.is_empty() || value == "N/A" {
                        continue;
                    }
                    // println!("out_time_ms={value}");
                    out_time_ms = Some(value.parse::<u64>().unwrap());
                }
                "speed" => {
                    if value.is_empty() || value == "N/A" {
                        continue;
                    }
                    // println!("speed={value}");
                    speed = Some(
                        value
                            .trim()
                            .strip_suffix('x')
                            .unwrap()
                            .parse::<f32>()
                            .unwrap(),
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
                            let _ = event_bus.publish(Event::TaskProgress { id, progress });
                            last_progress = progress;
                            out_time_ms = None;
                            speed = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    async fn send_error(event_bus: EventBus, id: u64, stderr: ChildStderr) -> () {
        let mut error_buf = String::new();
        if BufReader::new(stderr)
            .read_to_string(&mut error_buf)
            .await
            .unwrap()
            > 0
        {
            let _ = event_bus.publish(Event::TaskFailed {
                id,
                error: error_buf,
            });
        }
    }

    fn build_thumbnail_output<P: AsRef<Path>>(input: P, output: P) -> PathBuf {
        let mut out_path = output.as_ref().to_path_buf();
        let stem = input.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
        out_path.push(format!("{stem}-%04d"));
        out_path.set_extension("jpg");
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

    async fn run(&self, event_bus: EventBus) -> AppResult<()> {
        event_bus.publish(Event::TaskStarted { id: self.id })?;
        let metadata = metadata(&self.input).await?;
        let total_duration = metadata.duration();
        let mut child = self.build_thumbnail_command().spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::FfmpegError("Failed to capture FFmpeg stdout".to_string()))?;
        let start_time = Instant::now();
        let id = self.id;
        let stdout_handle = tokio::task::spawn(Self::send_progress(
            event_bus.clone(),
            id,
            stdout,
            start_time,
            total_duration,
        ));
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::FfmpegError("Failed to capture FFmpeg stdout".to_string()))?;
        let stderr_handle = tokio::task::spawn(Self::send_error(event_bus.clone(), id, stderr));
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;
        if let Ok(status) = child.wait().await {
            if !status.success() {
                return Err(AppError::FfmpegError(format!(
                    "FFmpeg exited with code: {}",
                    status.code().unwrap_or(-1)
                )));
            }

            let _ = event_bus.publish(Event::TaskCompleted { id });
        }
        Ok(())
    }
}
