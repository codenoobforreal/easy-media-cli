use crate::{
    cli::{build_task_list, run_tasks_with_ui},
    domain::{Fetcher as MetadataFetcher, Resolution},
    infra::{CapturingCommandRunner, EventBus, FileSystem},
    task::FfmpegTaskWrapper,
    tasks::VideoEncoder,
    ui::Renderer,
};
use anyhow::Result;
use clap::{Args, value_parser};
use std::{path::PathBuf, sync::Arc};

#[derive(Args, Debug)]
pub struct VeArgs {
    /// Input video file or directory (directories are processed recursively)
    #[arg(short, long)]
    input: PathBuf,
    /// Output directory; defaults to the parent directory of the input file
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Maximum directory scan depth 0–10, current directory if unset
    #[arg(short, long, value_parser = value_parser!(u8).range(0..=10))]
    depth: Option<u8>,
    /// Maximum output resolution (downscale only); default 1080p
    #[arg(short, long)]
    resolution: Option<Resolution>,
    /// Maximum output frame rate (capped if source exceeds); default 24
    #[arg(short, long, default_value_t = 24,value_parser = value_parser!(u8).range(1..=120))]
    fps: u8,
}

pub fn handle_encode_video(
    args: &VeArgs,
    event_bus: Arc<dyn EventBus>,
    command_runner: Arc<dyn CapturingCommandRunner>,
    metadata_fetcher: Arc<dyn MetadataFetcher>,
    file_system: Arc<dyn FileSystem>,
    renderer: Box<dyn Renderer>,
) -> Result<()> {
    let fs_clone = file_system.clone();

    let tasks = build_task_list(
        &args.input,
        args.depth,
        file_system.as_ref(),
        move |task_id, video| {
            let metadata = metadata_fetcher.fetch_metadata(&video)?;
            let encoder = VideoEncoder::new(
                task_id,
                video,
                args.output.as_deref(),
                args.resolution,
                args.fps,
                &metadata,
            )?;
            let wrapped = FfmpegTaskWrapper::new(
                encoder,
                command_runner.clone(),
                metadata_fetcher.clone(),
                fs_clone.clone(),
            );
            Ok(Arc::new(wrapped))
        },
    )?;

    drop(file_system);

    run_tasks_with_ui(tasks, event_bus, renderer)
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

    /// 解析 Ve 子命令参数，仅用于预期成功的正向场景
    fn parse_ve_args(cmd: &[&str]) -> Result<VeArgs> {
        let cli = Cli::try_parse_from(cmd)
            .with_context(|| format!("Failed to parse CLI args: {cmd:?}"))?;
        match cli.command {
            Commands::Ve(args) => Ok(args),
            Commands::Scs(..) => panic!("parse_ve_args only supports Ve subcommand"),
        }
    }

    fn run_ve_command(
        args: &VeArgs,
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

        handle_encode_video(
            args,
            bus_trait,
            runner_trait,
            fetcher_trait,
            fs_trait,
            renderer,
        )
    }

    mod args_parsing {
        use super::*;

        #[test]
        fn default_values_are_correct() -> Result<()> {
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "test.mp4"])?;
            assert_debug_snapshot!(args, @r#"
            VeArgs {
                input: "test.mp4",
                output: None,
                depth: None,
                resolution: None,
                fps: 24,
            }
            "#);
            Ok(())
        }

        #[test]
        fn all_short_args_parse_correctly() -> Result<()> {
            let args = parse_ve_args(&[
                "easy-media-cli",
                "ve",
                "-i",
                "/videos/input",
                "-o",
                "/output/encoded",
                "-d",
                "3",
                "-r",
                "1280x720",
                "-f",
                "30",
            ])?;
            assert_debug_snapshot!(args, @r#"
            VeArgs {
                input: "/videos/input",
                output: Some(
                    "/output/encoded",
                ),
                depth: Some(
                    3,
                ),
                resolution: Some(
                    Hd,
                ),
                fps: 30,
            }
            "#);
            Ok(())
        }

        #[test]
        fn all_long_args_parse_correctly() -> Result<()> {
            let args = parse_ve_args(&[
                "easy-media-cli",
                "ve",
                "--input",
                "input.mp4",
                "--output",
                "out_dir",
                "--depth",
                "5",
                "--resolution",
                "1920x1080",
                "--fps",
                "60",
            ])?;
            assert_debug_snapshot!(args, @r#"
            VeArgs {
                input: "input.mp4",
                output: Some(
                    "out_dir",
                ),
                depth: Some(
                    5,
                ),
                resolution: Some(
                    Fhd,
                ),
                fps: 60,
            }
            "#);
            Ok(())
        }

        #[test]
        fn fps_below_1_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "ve", "-i", "test.mp4", "-f", "0"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(), @"ValueValidation");
        }

        #[test]
        fn fps_above_120_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "ve", "-i", "test.mp4", "-f", "121"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(), @"ValueValidation");
        }

        #[test]
        fn depth_above_10_rejected() {
            let err = Cli::try_parse_from(["easy-media-cli", "ve", "-i", "test.mp4", "-d", "11"])
                .unwrap_err();
            assert_debug_snapshot!(err.kind(), @"ValueValidation");
        }

        #[test]
        fn invalid_resolution_format_rejected() {
            let err =
                Cli::try_parse_from(["easy-media-cli", "ve", "-i", "test.mp4", "-r", "invalid"])
                    .unwrap_err();
            assert_debug_snapshot!(err.kind(), @"ValueValidation");
        }

        #[test]
        fn resolution_zero_dimension_rejected() {
            let err =
                Cli::try_parse_from(["easy-media-cli", "ve", "-i", "test.mp4", "-r", "0x1080"])
                    .unwrap_err();
            assert_debug_snapshot!(err.kind(), @"ValueValidation");
        }

        #[test]
        fn missing_input_returns_missing_arg_error() {
            let err = Cli::try_parse_from(["easy-media-cli", "ve"]).unwrap_err();
            assert_debug_snapshot!(err.kind(), @"MissingRequiredArgument");
        }

        #[test]
        fn ve_help_text_stable_snapshot() {
            let err = Cli::try_parse_from(["easy-media-cli", "ve", "--help"]).unwrap_err();
            assert_debug_snapshot!(err.kind(), @"DisplayHelp");
        }
    }

    mod command_logic {
        use super::*;

        #[test]
        fn empty_video_dir_returns_no_video_error() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&vec![]);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            let err = run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer).unwrap_err();
            assert_debug_snapshot!(err, @r#""no video found in path: \n.""#);
            Ok(())
        }

        #[test]
        fn single_video_generates_one_task() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("demo.mp4")]);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;

            match bus
                .events()
                .iter()
                .find(|e| matches!(e, Event::TaskQueueStart { .. }))
                .unwrap()
            {
                Event::TaskQueueStart { total } => assert_eq!(*total, 1),
                _ => unreachable!(),
            }
            assert_eq!(fetcher.call_count(), 1);
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
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;

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
            assert_eq!(fetcher.call_count(), 3);
            Ok(())
        }

        #[test]
        fn custom_output_dir_creates_correct_path() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", ".", "-o", "/custom/output"])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;

            let created_dirs = fs.created_dirs();
            assert_debug_snapshot!(created_dirs, @r#"
               [
                   "/custom/output",
               ]
               "#);
            Ok(())
        }

        #[test]
        fn resolution_propagates_to_ffmpeg_scale() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", ".", "-r", "1280x720"])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            let args = runner.last_spawn_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = args[vf_idx + 1].to_string_lossy();
            assert_debug_snapshot!(filter_str,@r#""scale=1280:-2:flags=lanczos,fps=24""#);
            Ok(())
        }

        #[test]
        fn fps_propagates_to_ffmpeg_param() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", ".", "-f", "30"])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
            let args = runner.last_spawn_args();
            let r_idx = args.iter().position(|s| s == "-vf").unwrap();
            let filter_str = &args[r_idx + 1];
            assert_debug_snapshot!(filter_str,@r#""scale=1920:-2:flags=lanczos""#);
            Ok(())
        }

        #[test]
        fn task_ids_start_from_1_increment() -> Result<()> {
            let videos = vec![PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];
            let (bus, runner, fetcher, fs, renderer) = setup_test_suite(&videos);
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer)?;
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
            fs.set_dir_entries(
                ".",
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "permission denied",
                )),
            );
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            let err = run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer).unwrap_err();
            assert_debug_snapshot!(err, @r#"
               Custom {
                   kind: PermissionDenied,
                   error: "permission denied",
               }
               "#);
            Ok(())
        }

        #[test]
        fn metadata_fetch_error_propagates_upwards() -> Result<()> {
            let (bus, runner, fetcher, fs, renderer) =
                setup_test_suite(&vec![PathBuf::from("test.mp4")]);
            fetcher.set_err("metadata fetch failed");
            let args = parse_ve_args(&["easy-media-cli", "ve", "-i", "."])?;
            let err = run_ve_command(&args, &bus, &runner, &fetcher, &fs, renderer).unwrap_err();
            assert!(err.to_string().contains("metadata fetch failed"));
            Ok(())
        }
    }
}
