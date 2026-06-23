//! UI 统计数据结构

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    expected_total: usize,
    total: usize,
    pending: usize,
    running: usize,
    completed: usize,
    failed: usize,
    canceled: usize,
}

impl Stats {
    pub fn new(
        total: usize,
        pending: usize,
        running: usize,
        completed: usize,
        failed: usize,
        canceled: usize,
    ) -> Self {
        Self {
            expected_total: 0,
            total,
            pending,
            running,
            completed,
            failed,
            canceled,
        }
    }

    pub fn with_expected(
        expected_total: usize,
        total: usize,
        pending: usize,
        running: usize,
        completed: usize,
        failed: usize,
        canceled: usize,
    ) -> Self {
        Self {
            expected_total,
            total,
            pending,
            running,
            completed,
            failed,
            canceled,
        }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn running(&self) -> usize {
        self.running
    }

    pub fn completed(&self) -> usize {
        self.completed
    }

    pub fn failed(&self) -> usize {
        self.failed
    }

    pub fn canceled(&self) -> usize {
        self.canceled
    }

    pub fn expected_total(&self) -> usize {
        self.expected_total
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub fn sample_stats() -> Stats {
        Stats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn default_all_fields_zero() {
        let stats = Stats::default();
        assert_debug_snapshot!(stats,@"
        Stats {
            expected_total: 0,
            total: 0,
            pending: 0,
            running: 0,
            completed: 0,
            failed: 0,
            canceled: 0,
        }
        ");
    }

    #[test]
    fn new_constructor_sets_all_fields() {
        let stats = Stats::new(10, 2, 3, 4, 1, 0);
        assert_debug_snapshot!(stats,@"
        Stats {
            expected_total: 0,
            total: 10,
            pending: 2,
            running: 3,
            completed: 4,
            failed: 1,
            canceled: 0,
        }
        ");
    }

    #[test]
    fn partial_eq_distinguishes_different_values() {
        let a = Stats::new(5, 1, 1, 2, 1, 0);
        let b = Stats::new(5, 1, 1, 2, 1, 0);
        let c = Stats::new(5, 1, 1, 2, 0, 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn clone_preserves_full_state() {
        let original = Stats::new(10, 2, 3, 4, 1, 0);
        let cloned = original;
        assert_eq!(original, cloned);
    }
}
