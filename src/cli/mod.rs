//! cli 入口层
//! 程序入口、参数解析
//! 调用任务层能力

mod encode_video;
mod scene_cut_snap;

use crate::{
    common::collect_videos,
    domain::{Fetcher as MetadataFetcher, Task},
    infra::{
        CapturingCommandRunner, DefaultCommandRunner, DefaultFileSystem, DefaultMetadataFetcher,
        EventBus, FileSystem,
    },
    task::TaskManager,
    ui::{DefaultRenderer, Renderer, SyncUi},
};
use anyhow::{Result, anyhow, bail};
use clap::{Parser, Subcommand, value_parser};
pub use encode_video::{VeArgs, handle_encode_video};
pub use scene_cut_snap::{ScsArgs, handle_scene_cut_snap};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

#[derive(Parser, Debug)]
#[command(version, propagate_version = true)]
pub struct Cli {
    #[command(flatten)]
    global: GlobalConfig,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Parser)]
pub struct GlobalConfig {
    /// Terminal render interval in milliseconds
    #[arg(long, global = true, default_value_t = 100, value_parser = value_parser!(u64).range(1..=10000))]
    pub render_interval_ms: u64,

    /// Minimum percentage change to trigger a progress update
    #[arg(long, global = true, default_value_t = 1.0, value_parser = parse_progress_threshold)]
    pub progress_threshold: f32,
}

impl GlobalConfig {
    /// 返回解析的默认值而非 Default trait 的默认值（其不经过 clap）
    pub fn parser_default() -> Self {
        Self {
            render_interval_ms: 100,
            progress_threshold: 1.0,
        }
    }
}

fn parse_progress_threshold(s: &str) -> Result<f32> {
    let val = s
        .parse::<f32>()
        .map_err(|_| anyhow!("Invalid float: '{s}'"))?;
    if !(0.1..=10.0).contains(&val) {
        bail!("Progress threshold must be between 0.1 and 10.0, got {val}");
    }

    Ok(val)
}

#[derive(Subcommand, Debug)]
#[command(about)]
pub enum Commands {
    /// FFmpeg scene detection batch thumbnail generator
    #[allow(clippy::doc_markdown)]
    Scs(ScsArgs),
    /// Batch SVT-AV1 archival encoding with resolution/frame-rate caps
    Ve(VeArgs),
}

pub fn run_cli(event_bus: Arc<dyn EventBus>) -> Result<()> {
    let cli = Cli::parse();

    let file_system: Arc<dyn FileSystem> = Arc::new(DefaultFileSystem);
    let command_runner: Arc<dyn CapturingCommandRunner> = Arc::new(DefaultCommandRunner);
    let metadata_fetcher: Arc<dyn MetadataFetcher> =
        Arc::new(DefaultMetadataFetcher::new(command_runner.clone()));
    let terminal_renderer = Box::new(DefaultRenderer::default());

    match &cli.command {
        Commands::Scs(args) => handle_scene_cut_snap(
            args,
            event_bus,
            command_runner,
            metadata_fetcher,
            file_system,
            terminal_renderer,
            &cli.global,
        )?,
        Commands::Ve(args) => handle_encode_video(
            args,
            event_bus,
            command_runner,
            metadata_fetcher,
            file_system,
            terminal_renderer,
            &cli.global,
        )?,
    }

    Ok(())
}

/// 构建任务列表：收集视频并调用任务工厂生成任务
pub fn build_task_list<F>(
    input: &PathBuf,
    depth: Option<u8>,
    file_system: &dyn FileSystem,
    task_factory: F,
) -> Result<Vec<Arc<dyn Task>>>
where
    F: Fn(usize, PathBuf) -> Result<Arc<dyn Task>>,
{
    let depth = depth.or(Some(0));
    let videos = collect_videos(file_system, input, depth)?;
    if videos.is_empty() {
        bail!("no video found in path: \n{}", input.display());
    }

    let mut tasks = Vec::with_capacity(videos.len());
    for (idx, video) in videos.into_iter().enumerate() {
        let task_id = idx + 1;
        tasks.push(task_factory(task_id, video)?);
    }

    Ok(tasks)
}

/// 执行任务列表，驱动 UI 渲染，直到所有任务完成或被取消
pub fn run_tasks_with_ui(
    tasks: Vec<Arc<dyn Task>>,
    event_bus: Arc<dyn EventBus>,
    renderer: Box<dyn Renderer>,
    render_interval: Duration,
) -> Result<()> {
    let sync_ui = SyncUi::bind_event_bus(renderer, event_bus.as_ref(), render_interval)?;

    let task_manager = TaskManager::new(event_bus);
    task_manager.bind_shutdown_listener()?;

    let (finish_tx, finish_rx) = mpsc::channel::<Result<()>>();
    let task_manager_clone = task_manager.clone();
    thread::spawn(move || {
        let res = task_manager_clone.run_all(&tasks);
        let _ = finish_tx.send(res);
    });

    sync_ui.block_on_task_thread_finish_channel(&finish_rx)?;
    sync_ui.render_final(task_manager.is_cancelled())?;

    Ok(())
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::{
        domain::{Metadata as MediaMetadata, VideoStream},
        infra::{
            FileType, MockFileSystem,
            test_utils::{MockCommandRunner, MockEventBus, MockMetadataFetcher, exit_status},
        },
        ui::test_utils::MockRenderer,
    };
    use std::path::PathBuf;

    /// 统一构造测试套件：全套 Mock + 默认成功配置，两个子命令通用
    #[allow(clippy::type_complexity)]
    pub fn setup_test_suite(
        video_files: &Vec<PathBuf>,
    ) -> (
        Arc<MockEventBus>,
        Arc<MockCommandRunner>,
        Arc<MockMetadataFetcher>,
        Arc<MockFileSystem>,
        Box<dyn Renderer>,
    ) {
        let bus = Arc::new(MockEventBus::default());
        let runner = Arc::new(MockCommandRunner::default());
        let fetcher = Arc::new(MockMetadataFetcher::default());
        let fs = Arc::new(MockFileSystem::default());

        // 根路径标记为目录，解决 Path not found 问题
        fs.set_metadata(".", Ok(FileType::Dir));
        // 配置目录条目
        fs.set_dir_entries(".", Ok(video_files.clone()));
        // 每个视频标记为文件类型
        for path in video_files {
            fs.set_metadata(path, Ok(FileType::File));
        }

        let mut default_metadata = MediaMetadata::default();
        // 填充一个有效视频流，包含分辨率、帧率等核心字段
        default_metadata.video_streams.push(VideoStream {
            width: 1920,
            height: 1080,
            avg_frame_rate: Some(30.0),
            codec_name: "h264".to_string(),
            ..Default::default()
        });
        // 默认元数据获取成功（支持多次调用）
        fetcher.set_ok(default_metadata);
        // 默认命令执行成功（支持多次调用）
        runner.set_spawn_ok(vec![], vec![], exit_status(true));

        let renderer = Box::new(MockRenderer::default());
        (bus, runner, fetcher, fs, renderer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use insta::assert_debug_snapshot;

    mod cli_definition {
        use super::*;

        #[test]
        fn verify_cli_definition() {
            Cli::command().debug_assert();
        }
    }

    mod global_flags {
        use super::*;

        #[test]
        fn version_flag_works() {
            let err = Cli::try_parse_from(["easy-media-cli", "--version"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayVersion");
        }

        #[test]
        fn root_help_flag_works() {
            let err = Cli::try_parse_from(["easy-media-cli", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }

        #[test]
        fn unknown_subcommand_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "unknown"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"InvalidSubcommand");
        }

        #[test]
        fn no_subcommand_returns_error() {
            let err = Cli::try_parse_from(["easy-media-cli"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelpOnMissingArgumentOrSubcommand");
        }

        /// 快照测试：根命令帮助文本稳定
        #[test]
        fn root_help_text_stable_snapshot() {
            let err = Cli::try_parse_from(["easy-media-cli", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }
    }
}
