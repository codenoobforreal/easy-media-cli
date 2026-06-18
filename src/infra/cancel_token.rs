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
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MockCancelToken {
        cancelled: Mutex<bool>,
    }

    impl MockCancelToken {
        pub fn set_cancelled(&self, cancelled: bool) {
            *self.cancelled.lock().unwrap() = cancelled;
        }
    }

    impl CancelToken for MockCancelToken {
        fn cancel(&self) {
            *self.cancelled.lock().unwrap() = true;
        }

        fn is_cancelled(&self) -> bool {
            *self.cancelled.lock().unwrap()
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
