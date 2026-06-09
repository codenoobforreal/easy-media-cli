use anyhow::{Context, Result, anyhow};
use std::path::Path;
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

fn parse_ffprobe_metadata(output: &str) -> Result<Metadata> {
    let mut width = None;
    let mut duration = None;
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((k, v)) if !v.is_empty() && v != "N/A" => (k, v),
            _ => continue,
        };
        match key {
            "width" => {
                width = Some(
                    value
                        .parse::<u16>()
                        .with_context(|| format!("Failed to parse u16: {key}={value}"))?,
                );
            }
            "duration" => {
                // dbg!(&v);
                let secs = value
                    .parse::<f32>()
                    .with_context(|| format!("Failed to parse: {key}={value}"))?;
                duration = Some(Duration::from_secs_f32(secs));
                // dbg!(duration);
            }
            _ => {}
        }
    }
    Ok(Metadata::new(
        width.with_context(|| format!("The metadata information is missing the width value"))?,
        duration
            .with_context(|| format!("The metadata information is missing the duration value"))?,
    ))
}

fn build_metadata_command(input: &Path) -> Command {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error"]);
    cmd.args(FFPROBE_FORMAT);
    cmd.arg(input);
    cmd
}

pub async fn metadata(input: &Path) -> Result<Metadata> {
    let mut cmd = build_metadata_command(input);
    let output = cmd.output().await?;
    if output.status.success() {
        let metadata = parse_ffprobe_metadata(
            str::from_utf8(&output.stdout).with_context(|| format!("Stdout slice is not utf8"))?,
        )?;
        Ok(metadata)
    } else {
        let stderr =
            str::from_utf8(&output.stderr).with_context(|| format!("Stdout slice is not utf8"))?;
        Err(anyhow!(
            "Matadata command failed with code {}\n{}",
            output.status.code().unwrap_or(-1),
            stderr
        ))
    }
}
