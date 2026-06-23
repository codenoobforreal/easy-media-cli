use std::time::Duration;

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

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_raw_progress_new() {
        let rp = RawFfmpegProgress::new(25.5, 1_500_000);
        assert_debug_snapshot!(rp,@"
          RawFfmpegProgress {
              speed: 25.5,
              out_time_ms: 1500000,
          }
          ");
    }

    #[test]
    fn test_out_time_ms_duration() {
        let rp = RawFfmpegProgress::new(0.0, 1000);
        assert_debug_snapshot!(rp.out_time_ms_duration(),@"1ms");
        let rp = RawFfmpegProgress::new(0.0, 5_000_000);
        assert_debug_snapshot!(rp.out_time_ms_duration(), @"5s");
    }

    #[test]
    fn test_percentage_calculation() {
        let rp = RawFfmpegProgress::new(0.0, 0);
        assert_debug_snapshot!(rp.percentage(Duration::ZERO), @"100.0");

        let rp = RawFfmpegProgress::new(0.0, 0);
        let total = Duration::from_secs(10);
        assert_debug_snapshot!(rp.percentage(total), @"0.0");

        let rp = RawFfmpegProgress::new(0.0, 5_000_000);
        assert_debug_snapshot!(rp.percentage(total), @"50.0");

        let rp = RawFfmpegProgress::new(0.0, 15_000_000);
        assert_debug_snapshot!(rp.percentage(total), @"100.0");
    }

    #[test]
    fn test_average_speed() {
        let rp = RawFfmpegProgress::new(0.0, 1_000_000);
        assert_debug_snapshot!(rp.average_speed(Duration::ZERO), @"0.0");

        let rp = RawFfmpegProgress::new(0.0, 0);
        let elapsed = Duration::from_secs(1);
        assert_debug_snapshot!(rp.average_speed(elapsed), @"0.0");

        let rp = RawFfmpegProgress::new(0.0, 2_000_000);
        let elapsed = Duration::from_secs(1);
        assert_debug_snapshot!(rp.average_speed(elapsed), @"2.0");

        let rp = RawFfmpegProgress::new(0.0, 1_500_000);
        assert_debug_snapshot!(rp.average_speed(elapsed), @"1.5");
    }

    #[test]
    fn test_eta_calculation() {
        let total = Duration::from_secs(10);
        let elapsed = Duration::from_secs(2);

        let rp = RawFfmpegProgress::new(10.0, 10_000_000);
        assert_debug_snapshot!(rp.eta(total, elapsed), @"
          Some(
              0ns,
          )
          ");

        let rp = RawFfmpegProgress::new(0.0, 5_000_000);
        assert_eq!(rp.eta(total, elapsed), None);

        let rp = RawFfmpegProgress::new(-1.0, 5_000_000);
        assert_eq!(rp.eta(total, elapsed), None);

        let rp = RawFfmpegProgress::new(10.0, 5_000_000);
        assert_eq!(rp.eta(total, Duration::ZERO), None);

        let rp = RawFfmpegProgress::new(10.0, 5_000_000);
        assert_debug_snapshot!(rp.eta(total, elapsed), @"
          Some(
              2s,
          )
          ");

        let rp = RawFfmpegProgress::new(2.0, 0);
        let eta = rp.eta(total, Duration::from_secs(1));
        assert_eq!(eta, None);
    }
}
