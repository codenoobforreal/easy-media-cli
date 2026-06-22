use std::sync::atomic::{AtomicBool, Ordering};

pub trait CancelToken: Send + Sync {
    fn cancel(&self);
    fn is_cancelled(&self) -> bool;
}

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
pub mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Debug, Default)]
    pub struct MockCancelToken {
        cancelled: Mutex<bool>,
        // 延迟触发：当 call_count >= auto_cancel_after 时自动取消
        auto_cancel_after: AtomicUsize,
        call_count: AtomicUsize,
    }

    impl MockCancelToken {
        pub fn set_cancelled(&self, cancelled: bool) {
            *self.cancelled.lock().unwrap() = cancelled;
        }

        /// 设置在 `is_cancelled` 被调用 `calls` 次后自动变为取消状态
        pub fn cancel_after(&self, calls: usize) {
            self.auto_cancel_after.store(calls, Ordering::SeqCst);
        }
    }

    impl CancelToken for MockCancelToken {
        fn cancel(&self) {
            *self.cancelled.lock().unwrap() = true;
        }

        fn is_cancelled(&self) -> bool {
            // 如果已显式设置取消，直接返回
            if *self.cancelled.lock().unwrap() {
                return true;
            }
            // 否则按计数自动触发
            let threshold = self.auto_cancel_after.load(Ordering::SeqCst);
            if threshold > 0 {
                let current = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
                if current >= threshold {
                    // 触发取消
                    *self.cancelled.lock().unwrap() = true;
                    return true;
                }
            }
            false
        }
    }

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
