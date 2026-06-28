//! 缩放模式体积质量均衡的 svtav1 编码器
//!
//! ```bash
//! ffmpeg -h encoder=libsvtav1
//!```
//!
//! # 参考文档
//! - <https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/Ffmpeg.md>
//! - <https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/Parameters.md>
//! - <https://handbrake.fr/docs/en/1.10.0/workflow/adjust-quality.html>

use crate::{
    domain::{
        event::TaskResultPayload,
        media::{MediaMetadata, Orientation, Resolution},
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
    duration: Duration,
    original_size: u64,
    /// 恒定质量因子 (CRF)，值越小质量越高
    crf: u8,
    /// 帧率上限，若元数据帧率大于配置帧率则启用
    fps: Option<u8>,
    /// 缩放后的宽度（仅横向视频使用）
    scaled_width: Option<u16>,
    /// 缩放后的高度（仅纵向视频使用）
    scaled_height: Option<u16>,
}

impl VideoEncoder {
    /// 根据配置和输入视频元数据创建编码器实例。
    ///
    /// 会自动判断是否需要限制帧率，以及根据分辨率升降计算 CRF 与缩放尺寸。
    pub fn new(
        id: usize,
        input: impl Into<PathBuf>,
        output_dir: Option<&Path>,
        resolution: Option<Resolution>,
        fps: u8,
        metadata: &MediaMetadata,
    ) -> Result<Self> {
        let input = input.into();
        let output = Self::build_output_path(&input, output_dir)?;

        let metadata_fps = metadata.fps().ok_or_else(|| {
            if metadata.video_streams.is_empty() {
                anyhow!("Input file does not contain a video stream")
            } else {
                anyhow!(
                    "Video stream exists but frame rate (FPS) could not be determined from metadata"
                )
            }
        })?;

        // 如果原视频帧率高于配置的最大帧率，则启用帧率限制
        let fps = if metadata_fps > f64::from(fps) {
            Some(fps)
        } else {
            None
        };

        let (crf, scaled_width, scaled_height) =
            Self::compute_scaling_params(resolution.unwrap_or_default(), metadata)?;

        Ok(Self {
            id,
            input,
            output,
            duration: metadata.duration(),
            original_size: metadata.size(),
            crf,
            fps,
            scaled_width,
            scaled_height,
        })
    }

    /// 构建编码视频文件的输出路径
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

    /// 构建 SVT-AV1 编码参数。
    ///
    /// 设计目标：通用视频归档工具，在用户指定的分辨率与帧率上限内，尽可能压缩体积，同时将画质损失控制在可接受范围内
    ///
    /// # 参数解释
    /// - preset = 4：文档推荐的高效平衡点（范围 4~6），在编码速度与压缩效率之间取得最佳折衷，适合批量归档。
    /// - crf：自动策略遵循“越低分辨率可承受越高 CRF”的原则，在主观画质透明的前提下最大化体积压缩（参见 resolution_to_crf）。
    /// - gop（关键帧间隔）：10 倍帧率且不超过 300 帧，或固定 240 帧（约 10 秒），保证可寻址性的同时减少关键帧开销
    /// - 像素格式：强制 yuv420p10le，10 位编码可显著减少色带和条带效应，AV1 在 10 位模式下的编码效率更高，同等画质码率更低，文件体积增加可忽略不计（通常 <1%），是归档场景的必选项
    /// - SVT-AV1 专用参数（通过 svtav1-params 传递
    ///   - tune=0：启用主观视觉质量优化（VQ），提升纹理锐度与感知细节，归档视频的最终呈现应以人眼观感为准，而非 PSNR 等客观指标
    ///   - film-grain=8：检测并替换原始画面中的随机噪声为参数化合成颗粒，在保留胶片质感的前提下大幅降低噪声编码开销（节省 15%~25% 码率）；8 为实拍内容的通用推荐值，720p/1080p 下合成颗粒极难与原始噪声区分，对动画或极干净视频可下调至 4~6（但当前工具统一采用 8 以优先体积）
    ///   - enable-qm=1:qm-min=0:qm-max=15：启用非平坦量化矩阵，根据画面复杂度动态调整各频率系数的量化权重。在低 CRF（高质量）下，可在不损害主观画质的前提下额外压缩 5%~10% 体积；
    ///   - qm-min=0 允许最大程度的非均匀量化，提升高频系数压缩力度
    ///   - qp-scale-compress-strength=1：压缩同一 mini-GOP 内不同时间层之间的 QP 差异。减轻帧间质量波动，使画面观感更一致；保守级别（1）几乎无副作用，不会影响平均画质
    fn build_command_args(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();

        // 日志 & 进度
        args.extend(LOG_ERROR_ARGS.iter().map(OsString::from));
        args.extend(PROGRESS_ARGS.iter().map(OsString::from));

        // 输入文件
        args.extend([OsString::from("-i"), OsString::from(&self.input)]);

        // SVT-AV1 编码器及固定参数
        args.extend(CODEC_SVTAV1_ARGS.iter().map(OsString::from));
        args.extend([OsString::from(PRESET_SVTAV1_ARGS), OsString::from("4")]);

        // CRF 值（动态）
        args.extend([OsString::from("-crf"), OsString::from(self.crf.to_string())]);

        // GOP 大小（动态）
        args.extend([OsString::from("-g"), OsString::from(self.gop().to_string())]);

        // 像素格式 & 额外参数
        args.extend(PIX_FMT_10LE_ARGS.iter().map(OsString::from));
        args.extend(SVTAV1_PARAMS_ARGS.iter().map(OsString::from));

        // 音频流复制
        args.extend(COPY_AUDIO_ARGS.iter().map(OsString::from));

        // 可选的视频滤镜（动态）
        if let Some(vf_str) = self.video_filter() {
            args.extend([OsString::from("-vf"), OsString::from(vf_str)]);
        }

        // 输出文件
        args.push(OsString::from(&self.output));

        args
    }

    /// 计算编码缩放参数（CRF 和可选的缩放宽高）
    ///
    /// # 策略
    /// - 分辨率下降时（元数据分辨率 ≥ 配置分辨率）：根据视频朝向调整宽高，并使用配置分辨率对应的 CRF
    /// - 分辨率上升时（元数据分辨率 < 配置分辨率）：不缩放宽高，使用原始分辨率对应的 CRF
    ///
    /// # 返回值（元组），按顺序：
    /// - crf
    /// - 最终缩放宽度
    /// - 最终缩放高度
    fn compute_scaling_params(
        target_resolution: Resolution,
        metadata: &MediaMetadata,
    ) -> Result<(u8, Option<u16>, Option<u16>)> {
        let source_pixels = metadata
            .pixels()
            .ok_or_else(|| anyhow!("Input file does not contain a video stream"))?;

        let source_resolution = metadata
            .resolution()
            .ok_or_else(|| anyhow!("Could not determine video resolution from metadata"))?
            .map_err(|e| anyhow!("Invalid resolution: {e}"))?;

        let (effective_resolution, do_scale) = if source_pixels > target_resolution.pixels() {
            (target_resolution, true)
        } else {
            (source_resolution, false)
        };

        let crf = resolution_to_crf(effective_resolution);

        // 若需要缩小，计算缩放尺寸
        let (scaled_width, scaled_height) = if do_scale {
            let orientation = target_resolution.get_orientation();
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

        Ok((crf, scaled_width, scaled_height))
    }

    /// 构建视频滤镜字符串。
    ///
    /// # flags
    /// - ffmpeg 的 scale 滤镜默认采用 bicubic，其在缩小图像时可能会丢失过多的锐度，导致画面变“软”。Lanczos 能提供更锐利的输出，更符合归档对画质的底线要求
    /// - Lanczos 的计算复杂度高于双三次但远低于更极端的算法（如 sinc 或 lanczos 的更高瓣数），对于批量归档任务来说，编码耗时远大于缩放，这部分性能开销可以忽略不计
    /// - 虽然目标是压缩体积，但缩小分辨率是破坏性操作。使用 Lanczos 可以最大限度地保留原画面的纹理、线条等视觉信息，避免缩放后的画面变得模糊
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

    /// 计算关键帧间隔（GOP）。
    ///
    /// - 如果有帧率限制，则取 `fps * 10` 与 300 的较小值；
    /// - 否则默认 240（对应于 CLI 默认 fps 的 10 倍）。
    fn gop(&self) -> u16 {
        match self.fps {
            Some(fps) => min((u16::from(fps)) * 10, 300),
            None => 240,
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
        Some(self.duration)
    }

    #[allow(clippy::cast_precision_loss)]
    fn result_payload(&self, output_size: Option<u64>) -> Option<TaskResultPayload> {
        let output_size = output_size.unwrap_or_default();
        let size_change =
            (output_size as f64 - self.original_size as f64) / self.original_size as f64;
        Some(TaskResultPayload::VideoEncoder {
            output_path: self.output.clone(),
            size_bytes: output_size,
            size_change,
        })
    }
}

/// 根据分辨率推荐 CRF 值，当前是质量优先
fn resolution_to_crf(resolution: Resolution) -> u8 {
    match resolution.pixels() {
        p if p >= Resolution::Uhd.pixels() => 22, // 4K
        p if p >= Resolution::Qhd.pixels() => 24, // 1440p
        p if p >= Resolution::Fhd.pixels() => 28, // 1080p
        p if p >= Resolution::Hd.pixels() => 30,  // 720p
        _ => 32,
    }
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::domain::media::VideoStream;
    use insta::assert_debug_snapshot;

    fn sample_metadata(width: u16, height: u16, fps: f64) -> MediaMetadata {
        MediaMetadata {
            video_streams: vec![VideoStream {
                width,
                height,
                avg_frame_rate: Some(fps),
                ..VideoStream::default()
            }],
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
        let metadata = sample_metadata(1920, 1080, 30.0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24, &metadata).unwrap();
        // 输出路径格式：input-时间戳.mp4，我们无法预测时间戳，只检查后缀
        assert!(encoder.output.to_string_lossy().starts_with("input-"));
        assert!(encoder.output.to_string_lossy().ends_with(".mp4"));
        assert_eq!(encoder.fps, Some(24)); // 原 fps 30 > 24
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        // CRF: 1080p => 28
        assert_eq!(encoder.crf, 28);
    }

    #[test]
    fn test_video_encoder_new_no_fps_cap() {
        let metadata = sample_metadata(1920, 1080, 20.0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24, &metadata).unwrap();
        assert_eq!(encoder.fps, None); // 原 fps 20 < 24，不限制
    }

    #[test]
    fn test_video_encoder_new_with_target_resolution_upscale() {
        let metadata = sample_metadata(1280, 720, 30.0);
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            None,
            Some(Resolution::Fhd), // 目标 1920x1080
            24,
            &metadata,
        )
        .unwrap();
        // 源分辨率低于目标，不缩放
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        // CRF 基于原始分辨率 720p => 30
        assert_eq!(encoder.crf, 30);
    }

    #[test]
    fn test_video_encoder_new_portrait_orientation() {
        let metadata = sample_metadata(1080, 1920, 30.0); // 竖屏
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            None,
            Some(Resolution::Vfhd), // 目标 1080x1920
            24,
            &metadata,
        )
        .unwrap();
        // 源分辨率等于目标，不缩放
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, None);
        assert_eq!(encoder.crf, 28);
    }

    #[test]
    fn test_video_encoder_new_portrait_downscale() {
        let metadata = sample_metadata(2160, 3840, 30.0); // 4K 竖屏
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            None,
            Some(Resolution::Vfhd), // 目标 1080x1920
            24,
            &metadata,
        )
        .unwrap();
        assert_eq!(encoder.scaled_width, None);
        assert_eq!(encoder.scaled_height, Some(1920));
        assert_eq!(encoder.crf, 28);
    }

    #[test]
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn build_command_args() {
        let metadata = sample_metadata(1920, 1080, 30.0);
        let encoder = VideoEncoder::new(
            1,
            "input.mp4",
            Some(Path::new("/output")),
            None,
            24,
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
        assert_debug_snapshot!(args.join(OsStr::new(" ")),@r#""-v error -progress pipe:1 -i input.mp4 -c:v libsvtav1 -preset 4 -crf 28 -g 240 -pix_fmt yuv420p10le -svtav1-params tune=0:film-grain=8:enable-qm=1:qm-min=0:qm-max=15:qp-scale-compress-strength=1 -c:a copy -vf fps=24""#);
    }

    #[test]
    fn test_video_encoder_gop() {
        let metadata = sample_metadata(1920, 1080, 30.0);
        let encoder = VideoEncoder::new(1, "input.mp4", None, None, 24, &metadata).unwrap();
        // fps=24, gop = min(24*10, 300) = 240
        assert_eq!(encoder.gop(), 240);
        let encoder_no_fps = VideoEncoder::new(1, "input.mp4", None, None, 60, &metadata).unwrap();
        // fps 不限，默认 240
        assert_eq!(encoder_no_fps.gop(), 240);
    }

    #[test]
    fn test_resolution_to_crf() {
        assert_eq!(resolution_to_crf(Resolution::Uhd), 22);
        assert_eq!(resolution_to_crf(Resolution::Qhd), 24);
        assert_eq!(resolution_to_crf(Resolution::Fhd), 28);
        assert_eq!(resolution_to_crf(Resolution::Hd), 30);
        assert_eq!(
            resolution_to_crf(Resolution::Arbitrary {
                width: 640,
                height: 480
            }),
            32
        );
    }
}
