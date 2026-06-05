#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Completed,
    Failed,
}

impl Default for Status {
    fn default() -> Self {
        Self::Pending
    }
}
