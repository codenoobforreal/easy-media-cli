pub trait CancelToken: Send + Sync {
    fn cancel(&self);
    fn is_cancelled(&self) -> bool;
}
