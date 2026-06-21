//! 基础设施层：纯技术实现，无业务逻辑

mod cancel_token;
mod command_runner;
mod event_bus;
mod file_system;
#[cfg(test)]
mod test_helpers;

#[cfg(test)]
pub use cancel_token::tests::MockCancelToken;
pub use cancel_token::{CancelToken, DefaultCancelToken};
pub use command_runner::{
    CapturingCommandRunner, CapturingCommandRunnerExt, ChildGuard, DefaultCommandRunner,
    StreamingCommandRunnerExt,
};
pub use event_bus::{DefaultEventBus, EventBus, EventHandler};
#[cfg(test)]
pub use file_system::tests::MockFileSystem;
pub use file_system::{DefaultFileSystem, FileSystem, FileType};

#[cfg(test)]
pub use command_runner::tests::MockCommandRunner;
#[cfg(test)]
pub use event_bus::tests::MockEventBus;
#[cfg(test)]
pub use test_helpers::{exit_status, exit_status_terminated, exit_status_with_code};
