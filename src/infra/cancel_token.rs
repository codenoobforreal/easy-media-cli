use crate::domain::cancel_token::CancelToken;
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
