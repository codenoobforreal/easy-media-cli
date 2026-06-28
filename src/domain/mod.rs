//! 领域层
//! 职责：业务核心契约、纯数据模型、纯业务规则
//! 准入标准：零外部依赖，不涉及任何 IO、外部工具、系统调用；是整个项目的最底层（从依赖关系来看）

pub mod cancel_token;
pub mod event;
pub mod media;
pub mod progress;
pub mod task;
