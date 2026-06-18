use crate::ffmpeg_progress::{Progress, RawFfmpegProgress};
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ProgressTracker {
    last_progress: Progress,
    total_duration: Duration,
}

impl ProgressTracker {
    pub fn new(total_duration: Duration) -> Self {
        Self {
            last_progress: Progress::default(),
            total_duration,
        }
    }

    pub fn update(&mut self, raw: RawFfmpegProgress, elapsed: Duration) -> Option<Progress> {
        let progress = Progress::from_raw_progress(raw, self.total_duration, elapsed);

        if progress.should_update(&self.last_progress) {
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

    /// 初始化后内部状态为默认值，总时长正确传入
    #[test]
    fn test_tracker_initial_state() {
        let total = Duration::from_secs(10);
        let tracker = ProgressTracker::new(total);
        assert_eq!(tracker.last_progress(), Progress::default());
        assert_eq!(tracker.total_duration(), total);
    }

    /// 第一次更新：上一次为 0%，强制触发更新并刷新内部状态
    #[test]
    fn test_tracker_first_update_triggers() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10));
        let raw = RawFfmpegProgress::new(1.0, 500_000); // 5% 进度
        let result = tracker.update(raw, Duration::from_secs(1));

        assert!(result.is_some());
        let progress = result.unwrap();
        assert!(approx_eq(f64::from(progress.percentage()), 5.0, 0.001));
        // 内部状态已同步更新
        assert_eq!(tracker.last_progress(), progress);
    }

    /// 进度变化小于阈值时，不触发更新，内部状态保持不变
    #[test]
    fn test_tracker_small_change_does_not_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(100)); // 总时长 100s，1% = 1_000_000µs
        // 初始更新到 10%
        let raw1 = RawFfmpegProgress::new(1.0, 10_000_000);
        let prev = tracker.update(raw1, Duration::from_secs(10)).unwrap();

        // 变化 0.5%，小于默认阈值 1%，不更新
        let raw2 = RawFfmpegProgress::new(1.0, 10_500_000);
        let result = tracker.update(raw2, Duration::from_secs(11));
        assert!(result.is_none());
        // 内部状态保持原值
        assert_eq!(tracker.last_progress(), prev);
    }

    /// 进度变化达到阈值时，触发更新并刷新内部状态
    #[test]
    fn test_tracker_threshold_change_triggers_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(100));
        // 初始 10%
        let raw1 = RawFfmpegProgress::new(1.0, 10_000_000);
        tracker.update(raw1, Duration::from_secs(10)).unwrap();

        // 变化 1%，刚好达到阈值，触发更新
        let raw2 = RawFfmpegProgress::new(1.0, 11_000_000);
        let result = tracker.update(raw2, Duration::from_secs(11));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            11.0,
            0.001
        ));
    }

    /// 进度达到 100% 时强制更新，不受阈值限制
    #[test]
    fn test_tracker_100_percent_forces_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10));
        // 先到 99.5%
        let raw1 = RawFfmpegProgress::new(1.0, 9_950_000);
        tracker.update(raw1, Duration::from_secs(9)).unwrap();

        // 到 100%，差值 0.5% 小于阈值，但强制更新
        let raw2 = RawFfmpegProgress::new(1.0, 10_000_000);
        let result = tracker.update(raw2, Duration::from_secs(10));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            100.0,
            0.001
        ));
    }

    /// 总时长为 0 时，第一次更新直接返回 100%
    #[test]
    fn test_tracker_zero_total_duration() {
        let mut tracker = ProgressTracker::new(Duration::ZERO);
        let raw = RawFfmpegProgress::new(1.0, 0);
        let result = tracker.update(raw, Duration::from_secs(1));
        assert!(result.is_some());
        assert!(approx_eq(
            f64::from(result.unwrap().percentage()),
            100.0,
            0.001
        ));
    }

    /// 进度倒退时不触发更新，内部状态不回退
    #[test]
    fn test_tracker_regression_does_not_update() {
        let mut tracker = ProgressTracker::new(Duration::from_secs(10));
        let raw1 = RawFfmpegProgress::new(1.0, 5_000_000);
        let prev = tracker.update(raw1, Duration::from_secs(5)).unwrap();

        // 进度回退到 40%，不触发更新
        let raw2 = RawFfmpegProgress::new(1.0, 4_000_000);
        let result = tracker.update(raw2, Duration::from_secs(4));
        assert!(result.is_none());
        assert_eq!(tracker.last_progress(), prev);
    }
}
