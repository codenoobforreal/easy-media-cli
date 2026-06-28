//! 基础设施层
//! 领域接口的具体实现，对接系统能力与外部工具
//! 仅依赖领域层抽象，封装所有外部细节

mod cancel_token;
mod command_runner;
mod event_bus;
mod ffmpeg_progress;
mod file_system;
mod metadata_fetcher;

pub use cancel_token::DefaultCancelToken;
pub use command_runner::{
    CapturingCommandRunner, CapturingCommandRunnerExt, ChildGuard, ChildHandle, CommandOutput,
    CommandSpec, CommandStreams, DefaultCommandRunner, StreamingCommandRunner,
    StreamingCommandRunnerExt,
};
pub use event_bus::DefaultEventBus;
pub use ffmpeg_progress::{FfmpegProgressParser, ProgressTracker};
pub use file_system::{DefaultFileSystem, FileSystem, FileType, MockFileSystem};
pub use metadata_fetcher::{DefaultMetadataFetcher, FfprobeRawJson, convert_raw_to_metadata};

#[cfg(test)]
pub mod test_utils {

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    use std::process::ExitStatus;

    /// 构造成功/失败的退出状态；0成功1失败
    pub fn exit_status(success: bool) -> ExitStatus {
        #[allow(clippy::bool_to_int_with_if)]
        exit_status_with_code(if success { 0 } else { 1 })
    }

    /// 指定退出码的正常退出状态
    pub fn exit_status_with_code(code: i32) -> ExitStatus {
        #[cfg(unix)]
        {
            // Unix 正常退出：退出码左移 8 位
            ExitStatusExt::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            #[allow(clippy::cast_sign_loss)]
            ExitStatusExt::from_raw(code as u32)
        }
    }

    /// 进程异常终止状态（信号杀死/崩溃，无退出码）
    pub fn exit_status_terminated() -> ExitStatus {
        #[cfg(unix)]
        {
            // 模拟 SIGKILL(9) 终止，低 7 位为信号号
            ExitStatusExt::from_raw(9)
        }
        #[cfg(windows)]
        {
            exit_status_with_code(-1)
        }
    }
}
