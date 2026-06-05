use std::path::Path;

use crate::error::{AppError, AppResult};
use tokio::{process::Command, time::Duration};

const FFPROBE_FORMAT: &[&str] = &[
    "-show_entries",
    "stream:format",
    "-of",
    "default=noprint_wrappers=1",
];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Metadata {
    width: u16,
    duration: Duration,
}

impl Metadata {
    pub fn new(width: u16, duration: Duration) -> Self {
        Self { width, duration }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }
}

fn parse_ffprobe_metadata(output: &str) -> AppResult<Metadata> {
    let mut width = None;
    let mut duration = None;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((k, v)) = line.split_once('=') else {
            continue;
        };

        match k {
            "width" => {
                width = Some(
                    v.parse::<u16>()
                        .map_err(|e| AppError::FfmpegError(e.to_string()))?,
                );
            }
            "duration" => {
                let secs = v
                    .parse::<f32>()
                    .map_err(|e| AppError::FfmpegError(e.to_string()))?;
                duration = Some(Duration::from_secs_f32(secs));
            }
            _ => {}
        }
    }

    Ok(Metadata::new(
        width.ok_or_else(|| {
            AppError::FfmpegError("The metadata information is missing the width value".to_string())
        })?,
        duration.ok_or_else(|| {
            AppError::FfmpegError(
                "The metadata information is missing the duration value".to_string(),
            )
        })?,
    ))
}

fn build_metadata_command(input: &Path) -> Command {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error"]);
    cmd.args(FFPROBE_FORMAT);
    cmd.arg(input);
    cmd
}

pub async fn metadata(input: &Path) -> AppResult<Metadata> {
    let mut cmd = build_metadata_command(input);
    let output = cmd.output().await?;

    if output.status.success() {
        let metadata = parse_ffprobe_metadata(
            str::from_utf8(&output.stdout).map_err(|e| AppError::FfmpegError(e.to_string()))?,
        )?;

        Ok(metadata)
    } else {
        let stderr =
            str::from_utf8(&output.stderr).map_err(|e| AppError::FfmpegError(e.to_string()))?;

        Err(AppError::FfmpegError(format!(
            "Matadata command failed with code {}\n{}",
            output.status.code().unwrap_or(-1),
            stderr
        )))
    }
}
