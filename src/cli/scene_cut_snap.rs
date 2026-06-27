use crate::{
    cli::{GlobalConfig, build_task_list, run_tasks_with_ui},
    domain::Fetcher as MetadataFetcher,
    infra::{CapturingCommandRunner, EventBus, FileSystem},
    task::CommandTaskWrapper,
    tasks::ThumbnailGenerator,
    ui::Renderer,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, value_parser};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Args, Debug)]
pub struct ScsArgs {
    /// Path to an input video file or a directory. Directories are processed recursively
    #[arg(short, long)]
    input: PathBuf,
    /// Directory for output files. If not set, a subfolder named after the input video is automatically created alongside it
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Maximum recursion depth for directory scans (0–10). If not given, only the specified directory is processed (equivalent to depth 0)
    #[arg(short, long, value_parser = value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
    /// Scene detection sensitivity threshold (0.01 – 1.0)
    #[arg(short, long, default_value_t = 0.3, value_parser = parse_threshold)]
    threshold: f32,
    /// Thumbnail width in pixels (minimum 1). Height is scaled proportionally
    #[arg(short, long, value_parser = value_parser!(u16).range(1..))]
    width: Option<u16>,
}

fn parse_threshold(s: &str) -> Result<f32> {
    let val = s
        .parse::<f32>()
        .map_err(|_| anyhow!("Invalid float: '{s}'"))?;
    if !(0.01..=1.0).contains(&val) {
        bail!("Scene detection sensitivity threshold must be between 0.01 and 1.0, got {val}");
    }

    Ok(val)
}

pub fn handle_scene_cut_snap(
    args: &ScsArgs,
    event_bus: Arc<dyn EventBus>,
    command_runner: Arc<dyn CapturingCommandRunner>,
    metadata_fetcher: Arc<dyn MetadataFetcher>,
    file_system: Arc<dyn FileSystem>,
    renderer: Box<dyn Renderer>,
    global_config: &GlobalConfig,
) -> Result<()> {
    let render_interval = Duration::from_millis(global_config.render_interval_ms);
    let progress_threshold = global_config.progress_threshold;
    let fs_clone = file_system.clone();

    let tasks = build_task_list(
        &args.input,
        args.depth,
        file_system.as_ref(),
        move |task_id, video| {
            let metadata = metadata_fetcher.fetch_metadata(&video)?;
            let generator = ThumbnailGenerator::new(
                task_id,
                video,
                args.output.as_deref(),
                args.threshold,
                args.width,
                &metadata,
            )?;
            let wrapped = CommandTaskWrapper::new(
                generator,
                command_runner.clone(),
                fs_clone.clone(),
                render_interval,
                progress_threshold,
            );
            Ok(Arc::new(wrapped))
        },
    )?;

    drop(file_system);

    run_tasks_with_ui(tasks, event_bus, renderer, render_interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{Cli, Commands, test_utils::setup_test_suite},
        domain::Event,
        infra::{
            MockFileSystem,
            test_utils::{MockCommandRunner, MockEventBus, MockMetadataFetcher},
        },
    };
    use anyhow::{Context, Result};
    use clap::Parser;
    use insta::assert_debug_snapshot;
    use std::io;

    /// 解析 Scs 子命令参数，仅用于预期成功的正向场景
    fn parse_scs_args(cmd: &[&str]) -> Result<ScsArgs> {
        let cli = Cli::try_parse_from(cmd)
            .with_context(|| format!("Failed to parse CLI args: {cmd:?}"))?;
        match cli.command {
            Commands::Scs(args) => Ok(args),
            Commands::Ve(..) => panic!("parse_scs_args only supports Scs subcommand"),
        }
    }

    fn run_scs_command(
        args: &ScsArgs,
        bus: &Arc<MockEventBus>,
        runner: &Arc<MockCommandRunner>,
        fetcher: &Arc<MockMetadataFetcher>,
        fs: &Arc<MockFileSystem>,
        renderer: Box<dyn Renderer>,
    ) -> Result<()> {
        let bus_trait: Arc<dyn EventBus> = bus.clone();
        let runner_trait: Arc<dyn CapturingCommandRunner> = runner.clone();
        let fetcher_trait: Arc<dyn MetadataFetcher> = fetcher.clone();
        let fs_trait: Arc<dyn FileSystem> = fs.clone();
        let config = GlobalConfig::parser_default();

        handle_scene_cut_snap(
            args,
            bus_trait,
            runner_trait,
            fetcher_trait,
            fs_trait,
            renderer,
            &config,
        )
    }

    mod args_parsing {
        use super::*;

        #[test]
        fn default_threshold_is_3() -> Result<()> {
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "test.mp4"])?;
            assert_debug_snapshot!(args,@r#"
            ScsArgs {
                input: "test.mp4",
                output: None,
                depth: None,
                threshold: 0.3,
                width: None,
            }
            "#);
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
                "0.7",
                "-w",
                "480",
                "-d",
                "2",
            ])?;
            assert_debug_snapshot!(args,@r#"
            ScsArgs {
                input: "/videos/input",
                output: Some(
                    "/output/thumbs",
                ),
                depth: Some(
                    2,
                ),
                threshold: 0.7,
                width: Some(
                    480,
                ),
            }
            "#);

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
                "0.2",
                "--width",
                "1920",
                "--depth",
                "10",
            ])?;
            assert_debug_snapshot!(args,@r#"
            ScsArgs {
                input: "input.mp4",
                output: Some(
                    "out_dir",
                ),
                depth: Some(
                    10,
                ),
                threshold: 0.2,
                width: Some(
                    1920,
                ),
            }
            "#);

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

        /// 快照测试：Scs 子命令帮助文本稳定
        #[test]
        fn scs_help_text_stable_snapshot() {
            let err = Cli::try_parse_from(["easy-media-cli", "scs", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(),@"DisplayHelp");
        }
    }

    mod command_logic {
        use super::*;

        #[test]
        fn empty_video_dir_returns_no_video_error() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&vec![]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            let err = run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer).unwrap_err();
            assert_debug_snapshot!(err,@r#""no video found in path: \n.""#);
            Ok(())
        }

        #[test]
        fn single_video_generates_one_task() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("demo.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
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
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&videos);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            match bus
                .events()
                .iter()
                .find(|e| matches!(e, Event::TaskQueueStart { .. }))
                .unwrap()
            {
                Event::TaskQueueStart { total } => assert_eq!(*total, 3),
                _ => unreachable!(),
            }
            assert_eq!(runner.spawn_call_count(), 3);
            Ok(())
        }

        #[test]
        fn custom_output_dir_creates_correct_path() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args =
                parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-o", "/custom/output"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            let created_dirs = fs.created_dirs();
            assert_debug_snapshot!(created_dirs,@r#"
            [
                "/custom/output",
            ]
            "#);
            Ok(())
        }

        #[test]
        fn threshold_propagates_to_ffmpeg_filter() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-t", "0.8"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            let args = runner.last_spawn_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = args[vf_idx + 1].to_string_lossy();
            assert!(filter_str.contains("gt(scene\\,0.8)"));
            Ok(())
        }

        #[test]
        fn width_propagates_to_scale_parameter() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", ".", "-w", "640"])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            let args = runner.last_spawn_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = args[vf_idx + 1].to_string_lossy();
            assert!(filter_str.contains("640:-2"));
            Ok(())
        }

        #[test]
        fn task_ids_start_from_1_increment() -> Result<()> {
            let videos = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&videos);
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
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
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&vec![]);
            // 注入目录读取错误
            fs.set_dir_entries(
                ".",
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "permission denied",
                )),
            );
            let args = parse_scs_args(&["easy-media-cli", "scs", "-i", "."])?;
            let err = run_scs_command(&args, &bus, &runner, &fetcher, &fs, renderer).unwrap_err();
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
