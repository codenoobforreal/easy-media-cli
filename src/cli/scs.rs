use crate::{
    common::collect_videos,
    domain::Task,
    infra::{CapturingCommandRunner, EventBus, FileSystem},
    media_metadata::MetadataFetcher,
    task::{FfmpegTaskWrapper, TaskManager},
    tasks::ThumbnailGenerator,
    ui::{DefaultRenderer, Renderer, SyncUi},
};
use anyhow::{Result, bail};
use clap::{Args, value_parser};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Args, Debug)]
pub struct ScsArgs {
    /// Input video file or directory
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short, long)]
    /// Output dir; auto creates video-named subfolders if unset
    output: Option<PathBuf>,
    /// Scene threshold 0–10, divided by 10 for FFmpeg; lower = more thumbnails
    #[allow(clippy::doc_markdown)]
    #[arg(short, long, default_value_t = 3, value_parser = value_parser!(u8).range(0..=10))]
    threshold: u8,
    /// Thumbnail min width 1, height auto-scaled
    #[arg(short, long, value_parser = value_parser!(u16).range(1..))]
    width: Option<u16>,
    /// Scan depth limit 0–10, current directory if unset
    #[arg(short, long, value_parser = value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
}

pub fn handle_scs_command(
    args: &ScsArgs,
    event_bus: Arc<dyn EventBus>,
    command_runner: &Arc<dyn CapturingCommandRunner>,
    metadata_fetcher: &Arc<dyn MetadataFetcher>,
    file_system: &Arc<dyn FileSystem>,
) -> Result<()> {
    let depth = args.depth.or(Some(0));
    let videos = collect_videos(file_system.as_ref(), &args.input, depth)?;
    if videos.is_empty() {
        bail!("no video found in path: \n{}", args.input.display())
    }

    let mut tasks: Vec<Arc<dyn Task>> = Vec::with_capacity(videos.len());

    for (idx, video) in videos.into_iter().enumerate() {
        let task_id = idx + 1;
        let thumbnail_generator = ThumbnailGenerator::new(
            task_id,
            video,
            args.output.as_deref(),
            args.threshold,
            args.width,
        )?;

        let wrapped_task = FfmpegTaskWrapper::new(
            thumbnail_generator,
            command_runner.clone(),
            metadata_fetcher.clone(),
            file_system.clone(),
        );
        tasks.push(Arc::new(wrapped_task));
    }

    let terminal_renderer: Box<dyn Renderer> = Box::new(DefaultRenderer::default());
    let sync_ui = SyncUi::bind_event_bus(terminal_renderer, event_bus.as_ref())?;

    let task_manager = TaskManager::new(event_bus);
    task_manager.bind_shutdown_listener()?;

    let (task_thread_finish_tx, task_thread_finish_rx) = mpsc::channel::<Result<()>>();

    let task_manager_clone = task_manager.clone();
    thread::spawn(move || {
        let res = task_manager_clone.run_all(&tasks);
        let _ = task_thread_finish_tx.send(res);
    });

    sync_ui.block_on_task_thread_finish_channel(&task_thread_finish_rx)?;
    sync_ui.render_final(task_manager.is_cancelled())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{Cli, Commands},
        domain::Event,
        infra::{FileType, MockCommandRunner, MockEventBus, MockFileSystem, exit_status},
        media_metadata::{MediaMetadata, MockMetadataFetcher},
    };
    use anyhow::{Context, Result};
    use clap::{CommandFactory, Parser};
    use insta::assert_debug_snapshot;
    use std::io;

    /// 解析 Scs 子命令参数，失败时附带上下文
    fn parse_scs_args(cmd: &[&str]) -> Result<ScsArgs> {
        let cli =
            Cli::try_parse_from(cmd).with_context(|| format!("Failed to parse from {cmd:?}"))?;
        match cli.command {
            Commands::Scs(args) => Ok(args),
        }
    }

    /// 参数解析层测试
    mod args_parsing {
        use super::*;

        /// 官方标准校验：一次性递归校验全量 CLI 定义合法性: 捕获重复选项、参数冲突、非法 `value_parser`、错误默认值等编程错误
        #[test]
        fn verify_cli_definition() {
            Cli::command().debug_assert();
        }

        #[test]
        fn default_threshold_is_3() -> Result<()> {
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "test.mp4"])?;
            assert_eq!(args.threshold, 3);
            assert!(args.output.is_none());
            assert!(args.width.is_none());
            assert!(args.depth.is_none());
            Ok(())
        }

        #[test]
        fn all_short_args_parse_correctly() -> Result<()> {
            let args = parse_scs_args(&[
                "easy-media-cli",
                "scs",
                "-i",
                "/videos/input",
                "-o",
                "/output/thumbs",
                "-t",
                "7",
                "-w",
                "480",
                "-d",
                "2",
            ])?;

            assert_eq!(args.input, PathBuf::from("/videos/input"));
            assert_eq!(args.output, Some(PathBuf::from("/output/thumbs")));
            assert_eq!(args.threshold, 7);
            assert_eq!(args.width, Some(480));
            assert_eq!(args.depth, Some(2));
            Ok(())
        }

        #[test]
        fn all_long_args_parse_correctly() -> Result<()> {
            let args = parse_scs_args(&[
                "easy-media-cli",
                "scs",
                "--input",
                "input.mp4",
                "--output",
                "out_dir",
                "--threshold",
                "0",
                "--width",
                "1920",
                "--depth",
                "10",
            ])?;

            assert_eq!(args.threshold, 0);
            assert_eq!(args.width, Some(1920));
            assert_eq!(args.depth, Some(10));
            Ok(())
        }

        #[test]
        fn threshold_above_10_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs", "-i", "test.mp4", "-t", "11"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(),@"ValueValidation");
        }

        #[test]
        fn width_zero_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs", "-i", "test.mp4", "-w", "0"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(),@"ValueValidation");
        }

        #[test]
        fn depth_above_10_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs", "-i", "test.mp4", "-d", "11"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(),@"ValueValidation");
        }

        #[test]
        fn missing_input_returns_missing_arg_error() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"MissingRequiredArgument");
        }

        #[test]
        fn cli_version_flag_works() {
            let err = Cli::try_parse_from(["easy-media-cli", "--version"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayVersion");
        }

        #[test]
        fn scs_help_text_stable_snapshot() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }
    }

    mod scs_command {
        use super::*;

        /// 全套 Mock + 默认成功配置，保留具体 Mock 类型，支持调用观测方法
        fn setup_test_suite(
            video_files: &Vec<PathBuf>,
        ) -> (
            Arc<MockEventBus>,
            Arc<MockCommandRunner>,
            Arc<MockMetadataFetcher>,
            Arc<MockFileSystem>,
        ) {
            let bus = Arc::new(MockEventBus::default());
            let runner = Arc::new(MockCommandRunner::default());
            let fetcher = Arc::new(MockMetadataFetcher::default());
            let fs = Arc::new(MockFileSystem::default());

            fs.set_metadata(".", Ok(FileType::Dir));
            fs.set_dir_entries(".", Ok(video_files.to_owned()));
            for path in video_files {
                fs.set_metadata(path, Ok(FileType::File));
            }
            fetcher.set_ok(MediaMetadata::default());
            runner.set_spawn_ok(vec![], vec![], exit_status(true));

            (bus, runner, fetcher, fs)
        }

        /// 统一封装类型转换，调用 `handle_scs_command`，消除每个测试重复写 Arc 强转的样板代码
        fn run_scs_command(
            args: &ScsArgs,
            bus: &Arc<MockEventBus>,
            runner: &Arc<MockCommandRunner>,
            fetcher: &Arc<MockMetadataFetcher>,
            fs: &Arc<MockFileSystem>,
        ) -> Result<()> {
            let bus_trait: Arc<dyn EventBus> = bus.clone();
            let runner_trait: Arc<dyn CapturingCommandRunner> = runner.clone();
            let fetcher_trait: Arc<dyn MetadataFetcher> = fetcher.clone();
            let fs_trait: Arc<dyn FileSystem> = fs.clone();
            handle_scs_command(args, bus_trait, &runner_trait, &fetcher_trait, &fs_trait)
        }

        #[test]
        fn empty_video_dir_returns_no_video_error() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            let err = run_scs_command(&args, &bus, &runner, &fetcher, &fs).unwrap_err();
            assert_debug_snapshot!(err,@r#""no video found in path: \n.""#);
            Ok(())
        }

        #[test]
        fn single_video_generates_one_task() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![PathBuf::from("demo.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            match bus
                .events()
                .iter()
                .find(|e| matches!(e, Event::TaskQueueStart { .. }))
                .unwrap()
            {
                Event::TaskQueueStart { total } => assert_eq!(*total, 1),
                _ => unreachable!(),
            }
            Ok(())
        }

        #[test]
        fn multiple_videos_matches_task_count() -> Result<()> {
            let videos = vec![
                PathBuf::from("a.mp4"),
                PathBuf::from("b.mkv"),
                PathBuf::from("c.mov"),
            ];
            let (bus, runner, fetcher, fs) = setup_test_suite(&videos);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            match bus
                .events()
                .iter()
                .find(|e| matches!(e, Event::TaskQueueStart { .. }))
                .unwrap()
            {
                Event::TaskQueueStart { total } => assert_eq!(*total, 3),
                _ => unreachable!(),
            }
            // 双重校验：spawn 调用次数等于任务数
            assert_eq!(runner.spawn_call_count(), 3);
            Ok(())
        }

        #[test]
        fn custom_output_dir_creates_correct_path() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args =
                parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-o", "/custom/output"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            let created_dirs = fs.created_dirs.lock().unwrap();
            assert_debug_snapshot!(created_dirs,@r#"
            [
                "/custom/output",
            ]
            "#);
            Ok(())
        }

        #[test]
        fn threshold_propagates_to_ffmpeg_filter() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-t", "8"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            let args = runner.last_spawn_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = args[vf_idx + 1].to_string_lossy();
            assert!(filter_str.contains("gt(scene\\,0.8)"));
            Ok(())
        }

        #[test]
        fn width_propagates_to_scale_parameter() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-w", "640"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            let args = runner.last_spawn_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = args[vf_idx + 1].to_string_lossy();
            assert!(filter_str.contains("640:-2"));
            Ok(())
        }

        #[test]
        fn task_ids_start_from_1_increment() -> Result<()> {
            let videos = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
            let (bus, runner, fetcher, fs) = setup_test_suite(&videos);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs)?;
            let started_ids: Vec<usize> = bus
                .events()
                .iter()
                .filter_map(|e| match e {
                    Event::TaskStarted { metadata } => Some(metadata.id()),
                    _ => None,
                })
                .collect();
            assert_eq!(started_ids, vec![1, 2]);
            Ok(())
        }

        #[test]
        fn read_dir_error_propagates_upwards() -> Result<()> {
            let (bus, runner, fetcher, fs) = setup_test_suite(&vec![]);
            // 注入目录读取错误
            fs.set_dir_entries(
                ".",
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "permission denied",
                )),
            );
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            let err = run_scs_command(&args, &bus, &runner, &fetcher, &fs).unwrap_err();
            assert_debug_snapshot!(err,@r#"
            Custom {
                kind: PermissionDenied,
                error: "permission denied",
            }
            "#);
            Ok(())
        }
    }
}
