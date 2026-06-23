use crate::domain::CancelToken;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default)]
pub struct DefaultCancelToken {
    cancelled: AtomicBool,
}

impl CancelToken for DefaultCancelToken {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[derive(Debug, Default)]
    pub struct MockCancelToken {
        cancelled: AtomicBool,
        auto_cancel_after: AtomicUsize,
        call_count: AtomicUsize,
    }

    impl MockCancelToken {
        pub fn set_cancelled(&self, cancelled: bool) {
            self.cancelled.store(cancelled, Ordering::SeqCst);
        }

        /// 在 is_cancelled 被调用 `calls` 次后自动取消
        pub fn cancel_after(&self, calls: usize) {
            self.auto_cancel_after.store(calls, Ordering::SeqCst);
            self.call_count.store(0, Ordering::SeqCst);
        }
    }

    impl CancelToken for MockCancelToken {
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::SeqCst);
        }

        fn is_cancelled(&self) -> bool {
            if self.cancelled.load(Ordering::SeqCst) {
                return true;
            }
            let threshold = self.auto_cancel_after.load(Ordering::SeqCst);
            if threshold > 0 {
                let current = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
                if current >= threshold {
                    self.cancelled.store(true, Ordering::SeqCst);
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::test_utils::MockCancelToken;

    #[test]
    fn default_token_starts_not_cancelled() {
        let token = DefaultCancelToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancel_sets_state_to_cancelled() {
        let token = DefaultCancelToken::default();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn multiple_cancel_is_idempotent() {
        let token = DefaultCancelToken::default();
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn mock_token_set_cancelled_works() {
        let token = MockCancelToken::default();
        assert!(!token.is_cancelled());
        token.set_cancelled(true);
        assert!(token.is_cancelled());
        token.set_cancelled(false);
        assert!(!token.is_cancelled());
    }

    #[test]
    fn mock_token_cancel_method_works() {
        let token = MockCancelToken::default();
        token.cancel();
        assert!(token.is_cancelled());
    }
}
