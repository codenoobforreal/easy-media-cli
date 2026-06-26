//! ui 展示层
//! 终端渲染
//! 调用任务层能力

mod progress_bar;
mod render_scheduler;
mod renderer;
mod stats;
mod sync_ui;
mod task_state_store;

use progress_bar::render_progress_bar;
use render_scheduler::RenderScheduler;
pub use renderer::{DefaultRenderer, Renderer};
use stats::Stats;
pub use sync_ui::SyncUi;
use task_state_store::TaskStateStore;

/// terminal 渲染间隔
pub const SUCCESS_MSG: &str = "All tasks were processed successfully!";
pub const CANCEL_MSG: &str = "Tasks execution cancelled by user!";
pub const FAILED_LIST_TITLE: &str = "List of failed tasks:";
pub const RESULT_LIST_TITLE: &str = "Task results:";

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use render_scheduler::test_utils::sample_ui_scheduler;
    pub use renderer::test_utils::MockRenderer;
    pub use stats::test_utils::sample_stats;
}
