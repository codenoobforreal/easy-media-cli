use crate::infra::{Progress, RawFfmpegProgress};
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
        let progress = Progress::from_raw_progress(raw, self.total_duration, elapsed);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::approx_eq;

    #[test]
    fn test_tracker_initial_state() {
        let total = Duration::from_secs(10);
        let tracker = ProgressTracker::new(total, 1.0);
        assert_eq!(tracker.last_progress(), Progress::default());
        assert_eq!(tracker.total_duration(), total);
    }

    #[test]
    fn test_tracker_first_update_triggers() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10), 1.0);
        let raw = RawFfmpegProgress::new(1.0, 500_000);
        let result = tracker.update(raw, Duration::from_secs(1));

        assert!(result.is_some());
        let progress = result.unwrap();
        assert!(approx_eq(f64::from(progress.percentage()), 5.0, 0.001));
        // 内部状态已同步更新
        assert_eq!(tracker.last_progress(), progress);
    }

    #[test]
    fn test_tracker_small_change_does_not_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(100), 1.0);

        let raw1 = RawFfmpegProgress::new(1.0, 10_000_000);
        let prev = tracker.update(raw1, Duration::from_secs(10)).unwrap();

        let raw2 = RawFfmpegProgress::new(1.0, 10_500_000);
        let result = tracker.update(raw2, Duration::from_secs(11));
        assert!(result.is_none());
        // 内部状态保持原值
        assert_eq!(tracker.last_progress(), prev);
    }

    #[test]
    fn test_tracker_threshold_change_triggers_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(100), 1.0);
        let raw1 = RawFfmpegProgress::new(1.0, 10_000_000);
        tracker.update(raw1, Duration::from_secs(10)).unwrap();

        let raw2 = RawFfmpegProgress::new(1.0, 11_000_000);
        let result = tracker.update(raw2, Duration::from_secs(11));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            11.0,
            0.001
        ));
    }

    #[test]
    fn test_tracker_100_percent_forces_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10), 1.0);
        let raw1 = RawFfmpegProgress::new(1.0, 9_950_000);
        tracker.update(raw1, Duration::from_secs(9)).unwrap();

        let raw2 = RawFfmpegProgress::new(1.0, 10_000_000);
        let result = tracker.update(raw2, Duration::from_secs(10));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            100.0,
            0.001
        ));
    }

    #[test]
    fn test_tracker_zero_total_duration() {
        let mut tracker = ProgressTracker::new(Duration::ZERO, 1.0);
        let raw = RawFfmpegProgress::new(1.0, 0);
        let result = tracker.update(raw, Duration::from_secs(1));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            100.0,
            0.001
        ));
    }

    #[test]
    fn test_tracker_regression_does_not_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10), 1.0);
        let raw1 = RawFfmpegProgress::new(1.0, 5_000_000);
        let prev = tracker.update(raw1, Duration::from_secs(5)).unwrap();

        let raw2 = RawFfmpegProgress::new(1.0, 4_000_000);
        let result = tracker.update(raw2, Duration::from_secs(4));
        assert!(result.is_none());
        assert_eq!(tracker.last_progress(), prev);
    }
}
