//! 领域模型层：全项目核心业务抽象

mod event;
mod task;

pub use event::Event;
#[cfg(test)]
pub use task::tests::{MockTask, sample_test_metadata, sample_test_metadata_with_id_name};
pub use task::{Status, Task, TaskError, TaskMetadata};
