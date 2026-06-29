use crate::{
    domain::{event::TaskResultPayload, media::MediaMetadata, task::TaskConfig},
    infra::CommandSpec,
    task::command::CommandTask,
    tasks::{
        FPS_MODE_ARGS, LOG_ERROR_ARGS, OVERWRITE_ARGS, PROGRESS_ARGS, SKIP_FRAME_ARGS,
        VIDEO_QUALITY_ARGS,
    },
};
use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailGenerator {
    id: usize,
    input: PathBuf,
    output: PathBuf,
    scene_threshold: f32,
    width: Option<u16>,
    origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Origin {
    width: u16,
    duration: Duration,
}

impl Origin {
    pub fn new(width: u16, duration: Duration) -> Self {
        Self { width, duration }
    }
}

impl ThumbnailGenerator {
    pub fn new(
        id: usize,
        input: impl Into<PathBuf>,
        output_dir: Option<&Path>,
        scene_threshold: f32,
        width: Option<u16>,
        metadata: &MediaMetadata,
    ) -> Result<Self> {
        let input = input.into();
        let output = Self::build_output_path(&input, output_dir)?;

        let origin = Origin::new(
            metadata
                .width()
                .with_context(|| "Failed to retrive metadata width")?,
            metadata.duration(),
        );

        Ok(Self {
            id,
            input,
            output,
            scene_threshold,
            width,
            origin,
        })
    }

    /// 构建缩略图输出路径
    /// - 若指定输出目录：在该目录下直接生成 `文件名-%04d.jpg` 文件名的文件
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

    /// 构建命令参数列表
    /// # 关于滤镜
    /// FFmpeg 的 -vf 参数用逗号来分隔不同的滤镜（链），例如 `filter1,filter2`。如果某个滤镜的参数内部需要出现逗号，就必须用反斜杠 \ 对它进行转义，写成 `\,`，否则 FFmpeg 会错误地把这个逗号当成滤镜分隔符，导致解析失败
    pub fn build_command_args(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();

        // 日志 & 进度
        args.extend(LOG_ERROR_ARGS.iter().map(OsString::from));
        args.extend(SKIP_FRAME_ARGS.iter().map(OsString::from));
        args.extend(PROGRESS_ARGS.iter().map(OsString::from));
        // 输入文件
        args.extend([OsString::from("-i"), OsString::from(&self.input)]);
        // 视频滤镜
        args.extend([OsString::from("-vf"), self.video_filter()]);
        // 输出参数
        args.extend(FPS_MODE_ARGS.iter().map(OsString::from));
        args.extend([OsString::from(VIDEO_QUALITY_ARGS), OsString::from("2")]);
        args.push(OsString::from(OVERWRITE_ARGS));
        // 输出文件
        args.push(OsString::from(&self.output));

        args
    }

    fn video_filter(&self) -> OsString {
        let threshold = self.scene_threshold;
        match self.width {
            None => format!(
                "select=gt(scene\\,{threshold:.1}),scale=in_range=auto:out_range=full,format=yuv420p"
            )
            .into(),
            Some(width) => format!(
                "select=gt(scene\\,{threshold:.1}),scale={width}:-2:in_range=auto:out_range=full,format=yuvj420p"
            )
            .into(),
        }
    }
}

impl CommandTask for ThumbnailGenerator {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> String {
        self.file_name()
            .and_then(|s| s.to_str())
            .map_or("Generate thumbnail".to_string(), |s| {
                format!("Generate thumbnail: {s}")
            })
    }

    fn config(&self) -> TaskConfig {
        let scene = self.scene_threshold;
        let width = self.width.unwrap_or(self.origin.width);
        TaskConfig::ThumbnailGenerator { scene, width }
    }

    fn input(&self) -> &Path {
        &self.input
    }

    fn output(&self) -> Option<&Path> {
        Some(&self.output)
    }

    fn duration(&self) -> Option<Duration> {
        Some(self.origin.duration)
    }

    fn file_name(&self) -> Option<&OsStr> {
        self.input.file_name()
    }

    fn command_spec(&self) -> CommandSpec {
        CommandSpec::new("ffmpeg", self.build_command_args())
    }

    fn result_payload(&self, duration: Duration, _: Option<u64>) -> Option<TaskResultPayload> {
        let output_dir = self.output.parent().unwrap_or(&self.output).to_path_buf();
        Some(TaskResultPayload::ThumbnailGenerator {
            output_dir,
            duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::media::VideoStream;

    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_build_output_path_with_output() {
        let input = PathBuf::from("videos/test.mp4");
        let output_dir = PathBuf::from("/thumbnails");
        let output = ThumbnailGenerator::build_output_path(&input, Some(&output_dir)).unwrap();
        let expected = PathBuf::from("/thumbnails/test-%04d.jpg");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_build_output_path_without_output() {
        let input = PathBuf::from("videos/test.mp4");
        let output = ThumbnailGenerator::build_output_path(&input, None).unwrap();
        let expected = PathBuf::from("videos/test/test-%04d.jpg");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_video_filter() {
        let metadata = MediaMetadata {
            video_streams: vec![VideoStream {
                width: 1920,
                ..VideoStream::default()
            }],
            ..MediaMetadata::default()
        };

        let with_width_generator = ThumbnailGenerator::new(
            1,
            Path::new("/input.mp4"),
            Some(Path::new("/input/input-%04d.jpg")),
            0.3,
            Some(200),
            &metadata,
        )
        .unwrap();
        assert_debug_snapshot!(with_width_generator.video_filter(),@r#""select=gt(scene\\,0.3),scale=200:-2:in_range=auto:out_range=full,format=yuvj420p""#);

        let without_width_generator = ThumbnailGenerator::new(
            1,
            Path::new("/input.mp4"),
            Some(Path::new("/input/input-%04d.jpg")),
            0.3,
            None,
            &metadata,
        )
        .unwrap();
        assert_debug_snapshot!(without_width_generator.video_filter(),@r#""select=gt(scene\\,0.3),scale=in_range=auto:out_range=full,format=yuv420p""#);
    }

    #[test]
    fn test_build_command_args() {
        let metadata = MediaMetadata {
            video_streams: vec![VideoStream {
                width: 1920,
                ..VideoStream::default()
            }],
            ..MediaMetadata::default()
        };

        let generator = ThumbnailGenerator::new(
            1,
            Path::new("/input.mp4"),
            Some(Path::new("/input/input-%04d.jpg")),
            0.3,
            None,
            &metadata,
        )
        .unwrap();
        let mut args = generator.build_command_args();
        let output_path = args.pop().unwrap();
        assert_eq!(Path::new(&output_path), generator.output().unwrap());
        assert_debug_snapshot!(args.join(OsStr::new(" ")),@r#""-v error -skip_frame nokey -progress pipe:1 -i /input.mp4 -vf select=gt(scene\\,0.3),scale=in_range=auto:out_range=full,format=yuv420p -fps_mode vfr -q:v 2 -y""#);
    }
}
