use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawProgress {
    pub speed: f32,
    /// The actual unit of `out_time_ms` is not microseconds but nanoseconds.
    pub out_time_ms: u64,
}

impl RawProgress {
    pub fn new(speed: f32, out_time_ms: u64) -> Self {
        Self { speed, out_time_ms }
    }

    fn percentage(&self, total_duration: Duration) -> f32 {
        if total_duration.is_zero() {
            100.0
        } else {
            (self.out_time_ms_duration().div_duration_f32(total_duration) * 100.0).min(100.0)
        }
    }

    fn eta(&self, total_duration: Duration, elapsed: Duration) -> Option<Duration> {
        let percentage = self.percentage(total_duration);
        if percentage >= 100.0 {
            Some(Duration::ZERO)
        } else if self.speed <= 0.0 {
            None
        } else {
            let remaining = total_duration.saturating_sub(self.out_time_ms_duration());
            Some(remaining.div_f32(self.average_speed(elapsed) as f32))
        }
    }

    fn out_time_ms_duration(&self) -> Duration {
        Duration::from_micros(self.out_time_ms)
    }

    fn average_speed(&self, elapsed: Duration) -> u16 {
        if elapsed.is_zero() {
            0
        } else {
            self.out_time_ms_duration().div_duration_f32(elapsed) as u16
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Progress {
    percentage: f32,
    elapsed: Duration,
    eta: Option<Duration>,
}

impl Progress {
    const DEFAULT_UPDATE_THRESHOLD: f32 = 1.0;

    fn new(percentage: f32, elapsed: Duration, eta: Option<Duration>) -> Self {
        Self {
            percentage,
            elapsed,
            eta,
        }
    }

    pub fn from_raw_progress(
        ffmpeg_progress: RawProgress,
        total_duration: Duration,
        elapsed: Duration,
    ) -> Self {
        if total_duration.is_zero() {
            return Self {
                percentage: 100.0,
                elapsed,
                eta: Some(Duration::ZERO),
            };
        }

        Self::new(
            ffmpeg_progress.percentage(total_duration),
            elapsed,
            ffmpeg_progress.eta(total_duration, elapsed),
        )
    }

    pub fn should_update(&self, previous: &Self) -> bool {
        self.should_update_with_threshold(previous, Self::DEFAULT_UPDATE_THRESHOLD)
    }

    pub fn should_update_with_threshold(&self, previous: &Self, threshold: f32) -> bool {
        if self.percentage <= 0.0 || self.percentage >= 100.0 {
            return true;
        }
        (self.percentage - previous.percentage).abs() > threshold
    }

    pub fn percentage(&self) -> f32 {
        self.percentage
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn eta(&self) -> Option<Duration> {
        self.eta
    }
}
