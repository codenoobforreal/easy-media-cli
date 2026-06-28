//! `FFmpeg` 进度解析模块

use crate::domain::progress::Progress;
use anyhow::{Context, Result};
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ProgressTracker {
    last_progress: Progress,
    total_duration: Duration,
    threshold: f32,
}

impl ProgressTracker {
    pub fn new(total_duration: Duration, threshold: f32) -> Self {
        Self {
            last_progress: Progress::default(),
            total_duration,
            threshold,
        }
    }

    pub fn update(&mut self, raw: RawFfmpegProgress, elapsed: Duration) -> Option<Progress> {
        let progress = to_domain_progress(raw, self.total_duration, elapsed);

        if progress.should_update_with_threshold(&self.last_progress, self.threshold) {
            self.last_progress = progress;
            Some(progress)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn last_progress(&self) -> Progress {
        self.last_progress
    }

    #[cfg(test)]
    pub fn total_duration(&self) -> Duration {
        self.total_duration
    }
}

#[derive(Default)]
pub struct FfmpegProgressParser {
    out_time_ms: Option<u64>,
    speed: Option<f32>,
    total_size: Option<u64>,
}

impl FfmpegProgressParser {
    pub fn feed_line(&mut self, line: &str) -> Result<Option<RawFfmpegProgress>> {
        let trim_line = line.trim();
        if trim_line.is_empty() {
            return Ok(None);
        }

        let (key, value) = match trim_line.split_once('=') {
            Some((k, v)) if !v.is_empty() && v != "N/A" => (k, v),
            _ => return Ok(None),
        };

        match key {
            "total_size" => {
                self.total_size = Some(
                    value
                        .parse()
                        .with_context(|| "Failed to parse total_size")?,
                );
                Ok(None)
            }

            "out_time_ms" => {
                self.out_time_ms = Some(
                    value
                        .parse()
                        .with_context(|| format!("Failed to parse out_time_ms: {value}"))?,
                );
                Ok(None)
            }

            "speed" => {
                self.speed = Some(
                    value
                        .trim()
                        .strip_suffix('x')
                        .with_context(|| format!("Failed to parse speed: {value}"))?
                        .parse()
                        .with_context(|| format!("Failed to parse speed value: {value}"))?,
                );
                Ok(None)
            }

            "progress" => {
                let result = match (self.speed, self.out_time_ms) {
                    (Some(speed), Some(out_time_ms)) => {
                        let raw = RawFfmpegProgress::new(speed, out_time_ms);
                        self.out_time_ms = None;
                        self.speed = None;
                        Some(raw)
                    }
                    _ => None,
                };
                Ok(result)
            }

            _ => Ok(None),
        }
    }

    pub fn total_size(&self) -> Option<u64> {
        self.total_size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawFfmpegProgress {
    speed: f32,
    /// `out_time_ms` 的实际单位不是微秒，而是纳秒。
    out_time_ms: u64,
}

impl RawFfmpegProgress {
    pub fn new(speed: f32, out_time_ms: u64) -> Self {
        Self { speed, out_time_ms }
    }

    pub fn percentage(&self, total_duration: Duration) -> f32 {
        if total_duration.is_zero() {
            100.0
        } else {
            (self.out_time_ms_duration().div_duration_f32(total_duration) * 100.0).min(100.0)
        }
    }

    pub fn eta(&self, total_duration: Duration, elapsed: Duration) -> Option<Duration> {
        if self.percentage(total_duration) >= 100.0 {
            return Some(Duration::ZERO);
        }
        if self.speed <= 0.0 {
            return None;
        }
        let avg_speed = self.average_speed(elapsed);
        (avg_speed != 0.0).then(|| {
            total_duration
                .saturating_sub(self.out_time_ms_duration())
                .div_f32(avg_speed)
        })
    }

    fn average_speed(&self, elapsed: Duration) -> f32 {
        if elapsed.is_zero() {
            0.0
        } else {
            self.out_time_ms_duration().div_duration_f32(elapsed)
        }
    }

    fn out_time_ms_duration(&self) -> Duration {
        Duration::from_micros(self.out_time_ms)
    }

    #[cfg(test)]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    #[cfg(test)]
    pub fn out_time_ms(&self) -> u64 {
        self.out_time_ms
    }
}

fn to_domain_progress(
    raw: RawFfmpegProgress,
    total_duration: Duration,
    elapsed: Duration,
) -> Progress {
    if total_duration.is_zero() {
        return Progress::new(100.0, elapsed, Some(Duration::ZERO));
    }

    Progress::new(
        raw.percentage(total_duration),
        elapsed,
        raw.eta(total_duration, elapsed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::approx_eq;

    #[test]
    fn test_ffmpeg_progress_parser_full_cycle() {
        let mut parser = FfmpegProgressParser::default();

        assert!(parser.feed_line("out_time_ms=1000000").unwrap().is_none());
        assert!(parser.feed_line("speed=2.5x").unwrap().is_none());

        let raw = parser.feed_line("progress=continue").unwrap().unwrap();
        assert!(approx_eq(f64::from(raw.speed()), 2.5, 0.1));
        assert_eq!(raw.out_time_ms(), 1_000_000);
    }

    #[test]
    fn test_ffmpeg_progress_parser_ignore_invalid_lines() {
        let mut parser = FfmpegProgressParser::default();
        assert!(parser.feed_line("invalid").unwrap().is_none());
        assert!(parser.feed_line("out_time_ms=N/A").unwrap().is_none());
        assert!(parser.feed_line("speed=N/A").unwrap().is_none());
        assert!(parser.feed_line("").unwrap().is_none());
    }

    #[test]
    fn test_raw_ffmpeg_progress_percentage() {
        let raw = RawFfmpegProgress::new(2.0, 5_000_000);
        let total = Duration::from_secs(10);

        assert!(approx_eq(f64::from(raw.percentage(total)), 50.0, 0.1));

        let raw_zero = RawFfmpegProgress::new(0.0, 0);
        assert!(approx_eq(
            f64::from(raw_zero.percentage(Duration::ZERO)),
            100.0,
            0.1
        ));
    }

    #[test]
    fn test_raw_ffmpeg_progress_eta() {
        let total = Duration::from_mins(2);
        let elapsed = Duration::from_mins(1);
        let raw = RawFfmpegProgress::new(2.0, 60_000_000); // 60 秒已完成

        let eta = raw.eta(total, elapsed).unwrap();
        assert_eq!(eta, Duration::from_mins(1));

        let raw_done = RawFfmpegProgress::new(2.0, 120_000_000);
        assert_eq!(raw_done.eta(total, elapsed).unwrap(), Duration::ZERO);

        let raw_zero_speed = RawFfmpegProgress::new(0.0, 60_000_000);
        assert!(raw_zero_speed.eta(total, elapsed).is_none());
    }

    #[test]
    fn test_progress_tracker_total_size() {
        let mut parser = FfmpegProgressParser::default();
        parser.feed_line("total_size=1024000").unwrap();
        assert_eq!(parser.total_size(), Some(1_024_000));
    }
}
