use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Progress {
    percentage: f32,
    elapsed: Duration,
    eta: Option<Duration>,
}

impl Progress {
    pub fn new(percentage: f32, elapsed: Duration, eta: Option<Duration>) -> Self {
        Self {
            percentage,
            elapsed,
            eta,
        }
    }

    pub fn should_update_with_threshold(&self, previous: &Self, threshold: f32) -> bool {
        if self.percentage <= 0.0 || self.percentage >= 100.0 || previous.percentage == 0.0 {
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
mod tests {
    use super::*;

    #[test]
    fn test_should_update_with_threshold() {
        let prev = Progress::new(10.0, Duration::ZERO, None);
        let curr = Progress::new(11.0, Duration::ZERO, None);
        assert!(curr.should_update_with_threshold(&prev, 1.0));

        let curr2 = Progress::new(12.0, Duration::ZERO, None);
        assert!(curr2.should_update_with_threshold(&prev, 1.0));

        let zero = Progress::new(0.0, Duration::ZERO, None);
        assert!(zero.should_update_with_threshold(&prev, 1.0));

        let hundred = Progress::new(100.0, Duration::ZERO, None);
        assert!(hundred.should_update_with_threshold(&prev, 1.0));

        let prev_zero = Progress::new(0.0, Duration::ZERO, None);
        assert!(curr.should_update_with_threshold(&prev_zero, 1.0));
    }
}
