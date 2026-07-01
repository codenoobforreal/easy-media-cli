//! 缩放模式体积质量均衡的 svtav1 编码器
//!
//! ```bash
//! ffmpeg -h encoder=libsvtav1
//!```
//!
//! # 核心参数
//! - preset（编码预设）：制编码器花费多少计算资源来压缩视频。预设值越低，编码器会尝试更多复杂的工具，从而在相同画质下得到更小的文件，但耗时成倍增加；分辨率越高，原始信息越多，值得投入更多编码时间来换取体积缩减
//! - crf（恒定质量因子）：编码器会动态调整每帧的量化强度，努力让整段视频的主观观感保持一致。值越低，量化越精细（高质量），文件越大；反之文件变小；高分辨率下，单个像素瑕疵在屏幕上更小、更难察觉，因此可以使用更高的 CRF（更大压缩）而依然保持良好观感
//! - gop（关键帧间隔，GOP）：两个完整图像帧（I 帧）之间的帧数。I 帧独立编码，体积大；后续 P/B 帧只存差异，体积小。GOP 越长，文件越小，但拖动进度条时需要解码更多帧，响应变慢；按视频帧率计算，保证间隔在 5‑10 秒内
//! - pix_fmt（像素格式）：强制使用 10‑bit 色深、4:2:0 色度采样的像素格式。相比 8‑bit，10‑bit 能表示更多颜色层次，避免天空、渐变区域出现色带，同时编码器压缩效率略高（因为量化步长更精细）。SVT‑AV1 对 10‑bit 的编码速度惩罚微乎其微（仅极快预设下较明显）
//! - tune（调优模式）：0 是主观视觉质量优化（VQ），针对人眼感知优化，保留纹理锐度
//! - film-grain（胶片颗粒合成）：实拍摄影机常带有噪点，这些随机噪点非常消耗码率。开启此功能后，编码器会先对画面进行降噪（去除噪点），然后分析噪点特征，在解码时合成相似的颗粒叠加回画面。这样一来，码率节省明显，而观感几乎不变
//! - qp-scale-compress-strength（QP 压缩强度）：在 mini‑GOP 内，不同层级帧（如 I、P、B）会使用不同的量化参数（QP）。该参数会压缩各级帧之间的 QP 差距，使整组画面的质量更均匀，避免出现“关键帧清晰、后续帧模糊”的波动；1 是保守的压缩，几乎不损失平均画质，但大幅提升一致性
//!
//! # 参考文档
//! - <https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/Ffmpeg.md>
//! - <https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/Parameters.md>
//! - <https://handbrake.fr/docs/en/1.10.0/workflow/adjust-quality.html>

use crate::{
    domain::{
        event::TaskResultPayload,
        media::{MediaMetadata, Orientation, Resolution},
        task::TaskConfig,
    },
    infra::CommandSpec,
    task::command::CommandTask,
    tasks::{
        CODEC_SVTAV1_ARGS, COPY_AUDIO_ARGS, LOG_ERROR_ARGS, PIX_FMT_10LE_ARGS, PRESET_SVTAV1_ARGS,
        PROGRESS_ARGS, SVTAV1_PARAMS_ARGS,
    },
};
use anyhow::{Context, Result, anyhow};
use chrono::Local;
use std::{
    cmp::min,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VideoEncoder {
    id: usize,
    input: PathBuf,
    output: PathBuf,
    crf: u8,
    preset: u8,
    fps: Option<f64>,
    /// 缩放后的宽度（仅横向视频使用）
    scaled_width: Option<u16>,
    /// 缩放后的高度（仅纵向视频使用）
    scaled_height: Option<u16>,
    origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Origin {
    resolution: Resolution,
    fps: f64,
    duration: Duration,
    size: u64,
}

impl Origin {
    pub fn new(resolution: Resolution, fps: f64, duration: Duration, size: u64) -> Self {
        Self {
            resolution,
            fps,
            duration,
            size,
        }
    }
}

impl VideoEncoder {
    pub fn new(
        id: usize,
        input: impl Into<PathBuf>,
        output_dir: Option<&Path>,
        resolution: Option<Resolution>,
        fps: f64,
        metadata: &MediaMetadata,
    ) -> Result<Self> {
        let input = input.into();
        let output = Self::build_output_path(&input, output_dir)?;

        let origin_fps = metadata.fps().ok_or_else(|| {
            if metadata.video_streams.is_empty() {
                anyhow!("Input file does not contain a video stream")
            } else {
                anyhow!(
                    "Video stream exists but frame rate (FPS) could not be determined from metadata"
                )
            }
        })?;
        let origin_size = metadata
            .size()
            .with_context(|| "Missing size from metadata")?;
        let origin_resolution = metadata
            .resolution()
            .with_context(|| "Missing resolution from metadata")?;
        let origin_duration = metadata
            .duration()
            .with_context(|| "Missing duration from metadata")?;
        let origin = Origin::new(origin_resolution, origin_fps, origin_duration, origin_size);

        let fps = if origin_fps > fps { Some(fps) } else { None };

        let (crf, preset, scaled_width, scaled_height) =
            Self::compute_scaling_params(resolution.unwrap_or_default(), metadata)?;

        Ok(Self {
            id,
            input,
            output,
            crf,
            preset,
            fps,
            scaled_width,
            scaled_height,
            origin,
        })
    }

    /// 构建编码视频文件的输出路径
    ///
    /// - 若指定输出目录：在该目录下直接生成 `文件名-时间戳.mp4` 文件名的文件
    /// - 若未指定输出目录：在输入文件同级目录下，生成文件名与前者一致模式的文件
    fn build_output_path(input: &Path, output_dir: Option<&Path>) -> Result<PathBuf> {
        let file_stem = input
            .file_stem()
            .with_context(|| format!("Input path has no valid file stem: {}", input.display()))?;

        let mut output_base = if let Some(dir) = output_dir {
            dir.to_path_buf()
        } else {
            let parent_dir = input.parent().with_context(|| {
                format!("Input path has no parent directory: {}", input.display())
            })?;

            parent_dir.to_path_buf()
        };

        let sufix = Local::now().format("%y%m%d%H%M%S");
        let mut file_name = OsString::from(file_stem);
        file_name.push(format!("-{sufix}.mp4"));
        output_base.push(file_name);

        Ok(output_base)
    }

    fn build_command_args(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();

        args.extend(LOG_ERROR_ARGS.iter().map(OsString::from));
        args.extend(PROGRESS_ARGS.iter().map(OsString::from));
        args.extend([OsString::from("-i"), OsString::from(&self.input)]);
        args.extend(CODEC_SVTAV1_ARGS.iter().map(OsString::from));
        args.extend([
            OsString::from(PRESET_SVTAV1_ARGS),
            OsString::from(self.preset.to_string()),
        ]);
        args.extend([OsString::from("-crf"), OsString::from(self.crf.to_string())]);
        args.extend([OsString::from("-g"), OsString::from(self.gop().to_string())]);
        args.extend(PIX_FMT_10LE_ARGS.iter().map(OsString::from));
        args.extend(SVTAV1_PARAMS_ARGS.iter().map(OsString::from));
        args.extend(COPY_AUDIO_ARGS.iter().map(OsString::from));
        if let Some(vf_str) = self.video_filter() {
            args.extend([OsString::from("-vf"), OsString::from(vf_str)]);
        }
        args.push(OsString::from(&self.output));

        args
    }

    /// 计算由分辨率影响的参数（缩放）
    ///
    /// # 返回值（元组），按顺序：
    /// - crf
    /// - preset
    /// - 最终缩放宽度
    /// - 最终缩放高度
    fn compute_scaling_params(
        target_resolution: Resolution,
        metadata: &MediaMetadata,
    ) -> Result<(u8, u8, Option<u16>, Option<u16>)> {
        let source_pixels = metadata
            .pixels()
            .ok_or_else(|| anyhow!("Input file does not contain a video stream"))?;

        let source_resolution = metadata
            .resolution()
            .with_context(|| "Missing resolution from metadata")?;

        let (effective_resolution, do_scale) = if source_pixels > target_resolution.pixels() {
            (target_resolution, true)
        } else {
            (source_resolution, false)
        };

        let crf = resolution_to_crf(effective_resolution);
        let preset = resolution_to_preset(effective_resolution);

        let (scaled_width, scaled_height) = if do_scale {
            let orientation = source_resolution.get_orientation();
            match orientation {
                Orientation::Landscape => {
                    let width = target_resolution.get_primary_dimension();
                    (Some(width), None)
                }
                Orientation::Portrait => {
                    let height = target_resolution.get_primary_dimension();
                    (None, Some(height))
                }
            }
        } else {
            (None, None)
        };

        Ok((crf, preset, scaled_width, scaled_height))
    }

    fn video_filter(&self) -> Option<String> {
        let scale_str = match (self.scaled_width, self.scaled_height) {
            (Some(w), None) => Some(format!("scale={w}:-2:flags=lanczos")),
            (None, Some(h)) => Some(format!("scale=-2:{h}:flags=lanczos")),
            _ => None,
        };

        let fps_str = self.fps.map(|f| format!("fps={f}"));

        match (scale_str, fps_str) {
            (Some(scale), Some(fps)) => Some(format!("{scale},{fps}")),
            (None, Some(fps)) => Some(fps),
            (Some(scale), None) => Some(scale),
            _ => None,
        }
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn gop(&self) -> u16 {
        match self.fps {
            Some(f) => min(f as u16 * 10, 300),
            None => 300,
        }
    }
}

impl CommandTask for VideoEncoder {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> String {
        self.file_name()
            .and_then(|s| s.to_str())
            .map_or("Encode video".to_string(), |s| format!("Encode video: {s}"))
    }

    #[allow(
        clippy::cast_lossless,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    fn config(&self) -> TaskConfig {
        let resolution = match (self.scaled_width, self.scaled_height) {
            (Some(w), None) => {
                let src_w = self.origin.resolution.width() as f64;
                let src_h = self.origin.resolution.height() as f64;
                let h = ((w as f64 * src_h / src_w) / 2.0).round() as u16 * 2;
                Resolution::new(w, h).expect("Valid calculated resolution")
            }

            (None, Some(h)) => {
                let src_w = self.origin.resolution.width() as f64;
                let src_h = self.origin.resolution.height() as f64;
                let w = ((h as f64 * src_w / src_h) / 2.0).round() as u16 * 2;
                Resolution::new(w, h).expect("Valid calculated resolution")
            }

            _ => self.origin.resolution,
        };

        let fps = self.fps.unwrap_or(self.origin.fps);
        let preset = self.preset;
        let crf = self.crf;

        TaskConfig::VideoEncoder {
            resolution,
            preset,
            crf,
            fps,
        }
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

    fn command_spec(&self) -> CommandSpec {
        CommandSpec::new("ffmpeg", self.build_command_args())
    }

    fn duration(&self) -> Option<Duration> {
        Some(self.origin.duration)
    }

    #[allow(clippy::cast_precision_loss)]
    fn result_payload(
        &self,
        duration: Duration,
        output_size: Option<u64>,
    ) -> Option<TaskResultPayload> {
        let output_size = output_size.unwrap_or_default();
        let size_change = {
            if output_size == 0 {
                0.0
            } else {
                (output_size as f64 - self.origin.size as f64) / self.origin.size as f64
            }
        };
        Some(TaskResultPayload::VideoEncoder {
            output_path: self.output.clone(),
            size_bytes: output_size,
            size_change,
            duration,
        })
    }
}

fn resolution_to_crf(resolution: Resolution) -> u8 {
    match resolution.pixels() {
        p if p >= Resolution::Uhd.pixels() => 38, // 4K
        p if p >= Resolution::Qhd.pixels() => 34, // 1440p
        p if p >= Resolution::Fhd.pixels() => 32, // 1080p
        p if p >= Resolution::Hd.pixels() => 30,  // 720p
        _ => 26,
    }
}

fn resolution_to_preset(resolution: Resolution) -> u8 {
    match resolution.pixels() {
        p if p >= Resolution::Uhd.pixels() => 2, // 4K
        p if p >= Resolution::Qhd.pixels() => 3, // 1440p
        p if p >= Resolution::Fhd.pixels() => 4, // 1080p
        p if p >= Resolution::Hd.pixels() => 6,  // 720p
        _ => 8,
    }
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::domain::media::{MetadataFormat, VideoStream};
    use insta::assert_debug_snapshot;

    fn sample_metadata(
        width: u16,
        height: u16,
        fps: f64,
        duration: Duration,
        size: u64,
    ) -> MediaMetadata {
        MediaMetadata {
            video_streams: vec![VideoStream {
                width: Some(width),
                height: Some(height),
                avg_frame_rate: Some(fps),
                ..VideoStream::default()
            }],
            format: MetadataFormat {
                duration: Some(duration),
                size: Some(size),
                ..MetadataFormat::default()
            },
            ..MediaMetadata::default()
        }
    }

    #[test]
    fn build_output_path_with_output() {
        let with_output = VideoEncoder::build_output_path(
            &PathBuf::from("videos/test.mp4"),
            Some(&PathBuf::from("/encoder")),
        )
        .unwrap();
        let with_output_str = with_output.to_string_lossy().replace('\\', "/");
        assert!(
            with_output_str.starts_with("/encoder/test-"),
            "{}",
            with_output.display()
        );
        assert!(
            with_output
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4")),
            "{}",
            with_output.display()
        );
    }

    #[test]
    fn build_output_path_without_output() {
        let without_output =
            VideoEncoder::build_output_path(&PathBuf::from("videos/test.mp4"), None).unwrap();
        let without_output_str = without_output.to_string_lossy().replace('\\', "/");
        assert!(
            without_output_str.starts_with("videos/test-"),
            "{}",
            without_output.display()
        );
        assert!(
            without_output
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4")),
            "{}",
            without_output.display()
        );
    }

    #[test]
    fn test_video_encoder_new_basic() {
        let metadata = sample_metadata(1920, 1080, 30.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24.0, &metadata).unwrap();
        assert!(encoder.output.to_string_lossy().starts_with("input-"));
        assert!(encoder.output.to_string_lossy().ends_with(".mp4"));
        assert_eq!(encoder.fps, Some(24.0));
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        assert_eq!(encoder.crf, 32);
        assert_eq!(encoder.preset, 4);
    }

    #[test]
    fn test_video_encoder_new_no_fps_cap() {
        let metadata = sample_metadata(1920, 1080, 20.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24.0, &metadata).unwrap();
        assert_eq!(encoder.fps, None);
    }

    #[test]
    fn test_video_encoder_new_with_target_resolution_upscale() {
        let metadata = sample_metadata(1280, 720, 30.0, Duration::ZERO, 0);
        let encoder =
            VideoEncoder::new(1, "input.mp4", None, Some(Resolution::Fhd), 24.0, &metadata)
                .unwrap();
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        assert_eq!(encoder.crf, 30);
    }

    #[test]
    fn test_video_encoder_new_portrait_orientation() {
        let metadata = sample_metadata(1080, 1920, 30.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            None,
            Some(Resolution::Vfhd),
            24.0,
            &metadata,
        )
        .unwrap();
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        assert_eq!(encoder.crf, 32);
    }

    #[test]
    fn test_video_encoder_new_portrait_downscale() {
        let metadata = sample_metadata(2160, 3840, 30.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            None,
            Some(Resolution::Vfhd),
            24.0,
            &metadata,
        )
        .unwrap();
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, Some(1920));
        assert_eq!(encoder.crf, 32);
        assert_eq!(encoder.preset, 4);
    }

    #[test]
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn build_command_args() {
        let metadata = sample_metadata(1920, 1080, 30.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            Some(Path::new("/output")),
            None,
            24.0,
            &metadata,
        )
        .unwrap();

        let mut args = encoder.build_command_args();
        let output_path = args.pop().unwrap();
        assert!(
            output_path
                .to_string_lossy()
                .replace('\\', "/")
                .starts_with("/output/input-")
        );
        assert!(
            Path::new(&output_path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        );
        assert_debug_snapshot!(args.join(OsStr::new(" ")),@r#""-v error -progress pipe:1 -i input.mp4 -c:v libsvtav1 -preset 4 -crf 32 -g 240 -pix_fmt yuv420p10le -svtav1-params tune=0:film-grain=8:qp-scale-compress-strength=1 -c:a copy -vf fps=24""#);
    }

    #[test]
    fn test_video_encoder_gop() {
        let metadata = sample_metadata(1920, 1080, 30.0, Duration::ZERO, 0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24.0, &metadata).unwrap();
        assert_eq!(encoder.gop(), 240);
        let encoder_no_fps =
            VideoEncoder::new(1, "input.mp4", None, None, 60.0, &metadata).unwrap();
        assert_eq!(encoder_no_fps.gop(), 300);
    }

    #[test]
    fn test_config() {
        let metadata = sample_metadata(1920, 1080, 30.0, Duration::ZERO, 0);
        let encoder =
            VideoEncoder::new(1, "input.mp4", None, Some(Resolution::Hd), 24.0, &metadata).unwrap();
        assert_eq!(
            encoder.config(),
            TaskConfig::VideoEncoder {
                resolution: Resolution::Hd,
                preset: 6,
                crf: 30,
                fps: 24.0
            }
        );

        let metadata = sample_metadata(1440, 2560, 60.0, Duration::ZERO, 0);
        let encoder =
            VideoEncoder::new(1, "input.mp4", None, Some(Resolution::Hd), 30.0, &metadata).unwrap();
        assert_eq!(
            encoder.config(),
            TaskConfig::VideoEncoder {
                resolution: Resolution::Vhd,
                preset: 6,
                crf: 30,
                fps: 30.0
            }
        );
    }
}
