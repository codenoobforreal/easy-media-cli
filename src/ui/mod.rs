//! ui 展示层
//! 终端渲染
//! 调用任务层能力

pub mod progress_bar;
pub mod renderer;
pub mod state;
pub mod sync_ui;

/// terminal 渲染间隔
pub const SUCCESS_MSG: &str = "All tasks were processed successfully!";
pub const CANCEL_MSG: &str = "Tasks execution cancelled by user!";
pub const FAILED_LIST_TITLE: &str = "List of failed tasks:";
pub const RESULT_LIST_TITLE: &str = "Task results:";
