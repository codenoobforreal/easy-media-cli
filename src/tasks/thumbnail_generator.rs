use crate::task::FfmpegTask;
use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailGenerator {
    id: usize,
    input: PathBuf,
    output: PathBuf,
    scene_threshold: u8,
    width: Option<u16>,
}

impl ThumbnailGenerator {
    pub fn new(
        id: usize,
        input: impl Into<PathBuf>,
        output_dir: Option<&Path>,
        scene_threshold: u8,
        width: Option<u16>,
    ) -> Result<Self> {
        let input = input.into();
        let output = Self::build_output_path(&input, output_dir)?;
        Ok(Self {
            id,
            input,
            output,
            scene_threshold,
            width,
        })
    }

    /// 构建缩略图输出路径
    /// - 若指定输出目录：在该目录下直接生成 `文件名-%04d.jpg` 格式的文件
    /// - 若未指定输出目录：在输入文件同级目录下，创建与文件同名的子目录存放缩略图
    pub fn build_output_path(input: &Path, output_dir: Option<&Path>) -> Result<PathBuf> {
        let file_stem = input
            .file_stem()
            .with_context(|| format!("Input path has no valid file stem: {}", input.display()))?;

        let mut output_base = if let Some(dir) = output_dir {
            dir.to_path_buf()
        } else {
            let parent_dir = input.parent().with_context(|| {
                format!("Input path has no parent directory: {}", input.display())
            })?;

            let mut sub_dir = parent_dir.to_path_buf();
            sub_dir.push(file_stem);
            sub_dir
        };

        let mut file_name = OsString::from(file_stem);
        file_name.push("-%04d.jpg");
        output_base.push(file_name);

        Ok(output_base)
    }

    pub fn build_ffmpeg_args(&self) -> Vec<OsString> {
        let threshold: f64 = f64::from(self.scene_threshold) / 10.0;

        let video_filter_str: OsString = match self.width {
            None => format!(
                "select=gt(scene,{threshold:.1}),scale=in_range=auto:out_range=full,format=yuv420p",
            )
            .into(),
            Some(width) => format!(
                "select=gt(scene,{threshold:.1}),scale=in_range=auto:out_range=full,format=yuvj420p:{width}:-2"
            )
            .into(),
        };

        let args = vec![
            // 日志控制
            OsString::from("-v"),
            OsString::from("error"),
            OsString::from("-skip_frame"),
            OsString::from("nokey"),
            // 进度管道输出
            OsString::from("-progress"),
            OsString::from("pipe:1"),
            // 输入文件
            OsString::from("-i"),
            OsString::from(&self.input),
            // 视频滤镜
            OsString::from("-vf"),
            video_filter_str,
            // 帧率、画质参数
            OsString::from("-fps_mode"),
            OsString::from("vfr"),
            OsString::from("-q:v"),
            OsString::from("2"),
            // 覆盖输出 + 输出路径
            OsString::from("-y"),
            OsString::from(&self.output),
        ];

        args
    }
}

impl FfmpegTask for ThumbnailGenerator {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> Option<&str> {
        self.input.file_stem().and_then(|s| s.to_str())
    }

    fn input(&self) -> &Path {
        &self.input
    }

    fn output(&self) -> Option<&Path> {
        Some(&self.output)
    }

    fn file_name(&self) -> Option<&OsStr> {
        self.input.file_name()
    }

    fn build_args(&self) -> Vec<OsString> {
        self.build_ffmpeg_args()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::ExecutionMode;
    use insta::assert_debug_snapshot;

    mod build_output_path {
        use super::*;

        #[test]
        fn with_explicit_output_dir() {
            let input = Path::new("/videos/sample.mp4");
            let output_dir = Path::new("/output/thumbs");
            let result = ThumbnailGenerator::build_output_path(input, Some(output_dir)).unwrap();
            assert_eq!(result, Path::new("/output/thumbs/sample-%04d.jpg"));
        }

        #[test]
        fn without_output_dir_creates_sibling_subdir() {
            let input = Path::new("/videos/sample.mp4");
            let result = ThumbnailGenerator::build_output_path(input, None).unwrap();
            assert_eq!(result, Path::new("/videos/sample/sample-%04d.jpg"));
        }

        #[test]
        fn missing_file_stem_returns_error() {
            let input = Path::new("..");
            let err = ThumbnailGenerator::build_output_path(input, None).unwrap_err();
            assert!(err.to_string().contains("no valid file stem"));
        }

        #[test]
        fn no_parent_without_output_dir_returns_error() {
            let input = Path::new("/");
            let err = ThumbnailGenerator::build_output_path(input, None).unwrap_err();
            assert_debug_snapshot!(err,@"Input path has no valid file stem: /");
        }
    }

    mod build_ffmpeg_args {
        use super::*;

        fn make_task(width: Option<u16>) -> ThumbnailGenerator {
            ThumbnailGenerator::new(1, "/input/test.mp4", Some(Path::new("/output")), 5, width)
                .unwrap()
        }

        #[test]
        fn contains_all_base_arguments() {
            let task = make_task(None);
            let args: Vec<String> = task
                .build_args()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            assert_debug_snapshot!(args,@r#"
            [
                "-v",
                "error",
                "-skip_frame",
                "nokey",
                "-progress",
                "pipe:1",
                "-i",
                "/input/test.mp4",
                "-vf",
                "select=gt(scene,0.5),scale=in_range=auto:out_range=full,format=yuv420p",
                "-fps_mode",
                "vfr",
                "-q:v",
                "2",
                "-y",
                "/output\\test-%04d.jpg",
            ]
            "#);
        }

        #[test]
        fn scene_threshold_converts_to_decimal() {
            let task = make_task(None);
            let args = task.build_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let vf_str = args[vf_idx + 1].to_string_lossy();
            assert_debug_snapshot!(vf_str,@r#""select=gt(scene,0.5),scale=in_range=auto:out_range=full,format=yuv420p""#);
        }

        #[test]
        fn without_width_uses_auto_scale_format() {
            let task = make_task(None);
            let args = task.build_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let vf_str = args[vf_idx + 1].to_string_lossy();
            assert_debug_snapshot!(vf_str,@r#""select=gt(scene,0.5),scale=in_range=auto:out_range=full,format=yuv420p""#);
        }

        #[test]
        fn with_width_sets_fixed_width_scale() {
            let task = make_task(Some(320));
            let args = task.build_args();
            let vf_idx = args.iter().position(|s| s == "-vf").unwrap();
            let vf_str = args[vf_idx + 1].to_string_lossy();
            assert_debug_snapshot!(vf_str,@r#""select=gt(scene,0.5),scale=in_range=auto:out_range=full,format=yuvj420p:320:-2""#);
        }
    }

    mod trait_impl {
        use super::*;

        #[test]
        fn id_returns_configured_value() {
            let task = ThumbnailGenerator::new(42, "test.mp4", None, 5, None).unwrap();
            assert_eq!(task.id(), 42);
        }

        #[test]
        fn name_returns_file_stem() {
            let task = ThumbnailGenerator::new(1, "/videos/demo.mp4", None, 5, None).unwrap();
            assert_eq!(task.name(), Some("demo"));
        }

        #[test]
        fn execution_mode_is_streaming() {
            let task = ThumbnailGenerator::new(1, "test.mp4", None, 5, None).unwrap();
            assert_eq!(task.execution_mode(), ExecutionMode::Streaming);
        }

        #[test]
        fn needs_progress_is_true() {
            let task = ThumbnailGenerator::new(1, "test.mp4", None, 5, None).unwrap();
            assert!(task.needs_progress());
        }

        #[test]
        fn output_returns_generated_path() {
            let task =
                ThumbnailGenerator::new(1, "test.mp4", Some(Path::new("/out")), 5, None).unwrap();
            assert_eq!(task.output().unwrap(), Path::new("/out/test-%04d.jpg"));
        }
    }
}
