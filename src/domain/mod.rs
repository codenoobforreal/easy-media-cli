//! 领域层
//! 职责：业务核心契约、纯数据模型、纯业务规则
//! 准入标准：零外部依赖，不涉及任何 IO、外部工具、系统调用；是整个项目的最底层（从依赖关系来看）

mod cancel_token;
mod event;
mod media;
mod task;

pub use cancel_token::CancelToken;
pub use event::{Event, TaskResultPayload};
pub use media::{
    AudioStream, Fetcher, Format, Metadata, Orientation, Resolution, ResolutionError, VideoStream,
};
pub use task::{Status, Task, TaskError, TaskMetadata};

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use task::test_utils::{MockTask, sample_test_metadata, sample_test_metadata_with_id_name};
}
