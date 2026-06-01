use anyhow::{Context, Result, anyhow};
use std::time::Duration;

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

pub fn parse_raw_metadata(output: &str) -> Result<Metadata> {
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
                    v.parse()
                        .with_context(|| format!("Invalid width value '{}'", v))?,
                );
            }
            "duration" => {
                let secs = v
                    .parse::<f64>()
                    .with_context(|| format!("Invalid duration value '{}'", v))?;
                duration = Some(Duration::from_secs_f64(secs));
            }
            _ => {}
        }
    }

    Ok(Metadata::new(
        width.ok_or_else(|| anyhow!("The metadata information is missing the width value"))?,
        duration
            .ok_or_else(|| anyhow!("The metadata information is missing the duration value."))?,
    ))
}
