//! `FFmpeg` 进度解析模块

mod parser;
mod progress;
mod raw_progress;
mod tracker;

pub use parser::FfmpegProgressParser;
pub use progress::Progress;
pub use raw_progress::RawFfmpegProgress;
pub use tracker::ProgressTracker;

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use progress::test_utils::{make_progress, sample_progress};
}
