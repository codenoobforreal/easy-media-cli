use crate::infra::RawFfmpegProgress;
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Progress {
    percentage: f32,
    elapsed: Duration,
    eta: Option<Duration>,
}

impl Progress {
    const DEFAULT_UPDATE_THRESHOLD: f32 = 1.0;

    pub fn new(percentage: f32, elapsed: Duration, eta: Option<Duration>) -> Self {
        Self {
            percentage,
            elapsed,
            eta,
        }
    }

    pub fn from_raw_progress(
        ffmpeg_progress: RawFfmpegProgress,
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
        if previous.percentage == 0.0 {
            return true;
        }
        self.percentage - previous.percentage >= threshold
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

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub fn sample_progress() -> Progress {
        Progress::default()
    }

    pub fn make_progress(percent: f32, elapsed: Duration, eta: Option<Duration>) -> Progress {
        Progress::new(percent, elapsed, eta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::approx_eq;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_from_raw_progress() {
        // 总时长为 0 时直接返回 100%
        let rp = RawFfmpegProgress::new(10.0, 0);
        let elapsed = Duration::from_secs(5);
        let progress = Progress::from_raw_progress(rp, Duration::ZERO, elapsed);
        assert_debug_snapshot!(progress.percentage(), @"100.0");
        assert_eq!(progress.elapsed(), elapsed);
        assert_debug_snapshot!(progress.eta(), @"
                Some(
                    0ns,
                )
                ");

        // 正常进度 50%
        let rp = RawFfmpegProgress::new(10.0, 5_000_000);
        let total = Duration::from_secs(10);
        let elapsed = Duration::from_secs(2);
        let progress = Progress::from_raw_progress(rp, total, elapsed);
        assert_debug_snapshot!(progress.percentage(), @"50.0");
        assert_eq!(progress.elapsed(), elapsed);
        assert_debug_snapshot!(progress.eta(), @"
                Some(
                    2s,
                )
                ");

        // 初始进度 0%
        let rp = RawFfmpegProgress::new(10.0, 0);
        let progress = Progress::from_raw_progress(rp, total, Duration::ZERO);
        assert!(approx_eq(f64::from(progress.percentage()), 0.0, 0.001));
    }

    #[test]
    fn test_should_update_default_threshold() {
        // 差值小于阈值，不更新
        let prev = Progress::new(50.0, Duration::ZERO, None);
        let curr = Progress::new(50.5, Duration::ZERO, None);
        assert!(!curr.should_update(&prev));

        // 差值刚好等于阈值，更新
        let curr = Progress::new(51.0, Duration::ZERO, None);
        assert!(curr.should_update(&prev));

        // 差值大于阈值，更新
        let curr = Progress::new(51.1, Duration::ZERO, None);
        assert!(curr.should_update(&prev));
    }

    #[test]
    fn test_should_update_custom_threshold() {
        let prev = Progress::new(50.0, Duration::ZERO, None);
        let threshold = 2.0;

        let curr = Progress::new(51.5, Duration::ZERO, None);
        assert!(!curr.should_update_with_threshold(&prev, threshold));

        let curr = Progress::new(52.0, Duration::ZERO, None);
        assert!(curr.should_update_with_threshold(&prev, threshold));

        let curr = Progress::new(52.1, Duration::ZERO, None);
        assert!(curr.should_update_with_threshold(&prev, threshold));
    }

    #[test]
    fn test_should_update_boundary_cases() {
        // 上一次进度为 0 时强制更新
        let prev = Progress::new(0.0, Duration::ZERO, None);
        let curr = Progress::new(0.5, Duration::ZERO, None);
        assert!(curr.should_update(&prev));

        // 当前进度为 0 时强制更新
        let prev = Progress::new(5.0, Duration::ZERO, None);
        let curr = Progress::new(0.0, Duration::ZERO, None);
        assert!(curr.should_update(&prev));

        // 当前进度为 100% 时强制更新
        let prev = Progress::new(99.5, Duration::ZERO, None);
        let curr = Progress::new(100.0, Duration::ZERO, None);
        assert!(curr.should_update(&prev));

        // 进度倒退时不更新
        let prev = Progress::new(50.0, Duration::ZERO, None);
        let curr = Progress::new(48.0, Duration::ZERO, None);
        assert!(!curr.should_update(&prev));
    }

    #[test]
    fn test_progress_getters() {
        let percentage = 75.75;
        let elapsed = Duration::from_secs(15);
        let eta = Some(Duration::from_secs(8));
        let progress = Progress::new(percentage, elapsed, eta);
        assert!(approx_eq(
            f64::from(progress.percentage()),
            f64::from(percentage),
            0.001
        ));
        assert_eq!(progress.elapsed(), elapsed);
        assert_eq!(progress.eta(), eta);
    }
}
