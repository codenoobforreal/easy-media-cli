use crate::domain::Resolution;
use anyhow::{Result, anyhow};
use std::time::Duration;

/// 媒体元数据
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub format: Format,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    // pub format_tags: HashMap<String, String>,
}

/// TODO 当前实现默认选取每种流的第一个流作为结果返回，该方式不一定准确
impl Metadata {
    pub fn new(
        format: Format,
        video_streams: Vec<VideoStream>,
        audio_streams: Vec<AudioStream>,
    ) -> Self {
        Self {
            format,
            video_streams,
            audio_streams,
        }
    }

    pub fn duration(&self) -> Duration {
        self.format.duration
    }

    pub fn width(&self) -> Option<u16> {
        self.video_streams.first().map(|s| s.width)
    }

    pub fn height(&self) -> Option<u16> {
        self.video_streams.first().map(|s| s.height)
    }

    pub fn pixels(&self) -> Option<u32> {
        self.video_streams
            .first()
            .map(|s| u32::from(s.width) * u32::from(s.height))
    }

    pub fn resolution(&self) -> Option<Result<Resolution>> {
        self.video_streams
            .first()
            .map(|s| Resolution::new(s.width, s.height).map_err(|e| anyhow!(e)))
    }

    /// 平均帧率
    pub fn fps(&self) -> Option<f64> {
        self.video_streams.first().and_then(|s| s.avg_frame_rate)
    }
}

/// 媒体元数据中的视频流
#[derive(Debug, Clone, Default)]
pub struct VideoStream {
    /// 编码器简称
    pub codec_name: String,
    /// 编码器完整标准名称
    pub codec_long_name: String,
    /// 编码器四字符标识(FourCC)，属于封装层编码标识
    pub codec_tag_string: String,
    /// 播放画面的宽尺寸
    pub width: u16,
    /// 播放画面的高尺寸
    pub height: u16,
    // pub coded_width: u16,
    // pub coded_height: u16,
    /// 像素格式，定义视频帧的色彩编码、采样方式与内存存储排布
    pub pix_fmt: String,
    /// 是否采用AVC封装格式
    pub is_avc: bool,
    /// 标称/实时帧率：流预设帧率
    pub r_frame_rate: Option<f64>,
    /// 实际平均帧率，总帧数除以总时长计算得出，反映真实播放帧率
    pub avg_frame_rate: Option<f64>,
    /// 可读起始时间
    pub start_time: Duration,
    /// 总播放时长
    pub duration: Duration,
    /// 流码率
    pub bit_rate: u64,
    /// 流总帧数
    pub nb_frames: u32,
    /// 由处置中的 `default` 字段决定；1 表示真，0 表示假
    pub is_default: bool,
    // pub tags: HashMap<String, String>,
}

/// 媒体元数据中的音频流
#[derive(Debug, Clone, Default)]
pub struct AudioStream {
    pub codec_name: String,
    pub codec_long_name: String,
    pub codec_tag_string: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub channel_layout: String,
    pub r_frame_rate: Option<f64>,
    pub avg_frame_rate: Option<f64>,
    pub start_time: Duration,
    pub duration: Duration,
    pub bit_rate: u64,
    pub nb_frames: u32,
    /// 由处置中的 `default` 字段决定；1 表示真，0 表示假
    pub is_default: bool,
    // pub tags: HashMap<String, String>,
}

/// 媒体元数据中的容器格式
#[derive(Debug, Clone, Default)]
pub struct Format {
    /// 媒体文件完整路径与文件名
    pub filename: String,
    /// FFmpeg解封装器简称；逗号分隔列表代表该解封装器支持的全部封装格式
    ///
    /// `json` 实际字段名为 `format_name`
    pub name: String,
    /// 封装格式完整官方名称
    ///
    /// `json` 实际字段名为 `format_long_name`
    pub long_name: String,
    /// 整个文件的起始时间，取所有流中最早的起始时间
    pub start_time: Duration,
    /// 文件总播放时长，取所有流中最长时长作为封装容器总时长
    pub duration: Duration,
    /// 媒体文件总大小，单位字节
    pub size: u64,
    /// 文件整体平均码率，文件总大小除以总时长计算，包含音视频数据与封装包头开销，单位bps
    pub bit_rate: u64,
}
