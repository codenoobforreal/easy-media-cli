//! 业务任务层
//! 逐个具体业务任务的实现
//! 依赖任务编排能力与领域模型，实现具体业务逻辑

mod media_metadata_getter;
mod thumbnail_generator;
mod video_encoder;

pub use media_metadata_getter::MediaMetadataGetter;
pub use thumbnail_generator::ThumbnailGenerator;
pub use video_encoder::VideoEncoder;

/// 日志级别：只输出错误信息
pub const LOG_ERROR_ARGS: &[&str] = &["-v", "error"];
/// 将编码进度信息写入管道（pipe:1），方便外部读取
pub const PROGRESS_ARGS: &[&str] = &["-progress", "pipe:1"];
/// 忽略关键帧，强制全帧检测（用于场景检测相关）
pub const SKIP_FRAME_ARGS: &[&str] = &["-skip_frame", "nokey"];
/// 输出帧率模式：可变帧率（vfr）
pub const FPS_MODE_ARGS: &[&str] = &["-fps_mode", "vfr"];
/// 视频质量参数（-q:v）
pub const VIDEO_QUALITY_ARGS: &str = "-q:v";
/// 覆盖输出文件（不询问确认）
pub const OVERWRITE_ARGS: &str = "-y";

/// SVT-AV1 编码器选择
pub const CODEC_SVTAV1_ARGS: &[&str] = &["-c:v", "libsvtav1"];
/// SVT-AV1 预设值（这里固定为 4）
pub const PRESET_SVTAV1_ARGS: &str = "-preset";
/// 10bit YUV 4:2:0 像素格式
pub const PIX_FMT_10LE_ARGS: &[&str] = &["-pix_fmt", "yuv420p10le"];
/// SVT-AV1 扩展参数（tune、film-grain 等）
pub const SVTAV1_PARAMS_ARGS: &[&str] = &[
    "-svtav1-params",
    "tune=0:film-grain=8:enable-qm=1:qm-min=0:qm-max=15:qp-scale-compress-strength=1",
];
/// 音频流直接复制，不重新编码
pub const COPY_AUDIO_ARGS: &[&str] = &["-c:a", "copy"];

/// 探测时输出的条目：节目、格式、流、章节
pub const SHOW_ENTRIES_ARGS: &[&str] = &["-show_entries", "program:format:stream:chapter"];
/// 输出格式指定为 JSON
pub const OUTPUT_FORMAT_JSON_ARGS: &[&str] = &["-of", "json"];

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use video_encoder::test_utils::make_video_encoder_metadata;
}
