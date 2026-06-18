//! 原始 JSON 反序列化结构

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct FfprobeRawJson {
    #[serde(default)]
    pub streams: Vec<Stream>,
    #[serde(default)]
    pub format: Format,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct Stream {
    /// 媒体文件内的流索引号，从0开始计数
    pub index: u8,

    /// 编码器简称
    pub codec_name: String,

    /// 编码器完整标准名称
    pub codec_long_name: String,

    /// 编码档次/配置集：定义编码可用的算法集合，直接影响压缩效率与解码复杂度
    pub profile: String,

    /// 媒体流类型，`video` 代表视频流，`audio` 代表音频流，另有字幕流 `subtitles`、数据流 `data` 等其他类型
    pub codec_type: String,

    /// 编码器四字符标识(FourCC)，属于封装层编码标识
    pub codec_tag_string: String,

    /// `codec_tag_string` 对应的十六进制数值
    pub codec_tag: String,

    /// 遵循RFC 6381规范的MIME编码参数字符串，用于网页、HTTP场景下精准描述编码参数
    pub mime_codec_string: String,

    /// 视频分辨率，播放画面的宽高尺寸
    pub width: Option<u16>,
    pub height: Option<u16>,

    /// 编码分辨率：编码器实际处理帧的原始宽高
    pub coded_width: Option<u16>,
    pub coded_height: Option<u16>,

    /// GOP图像组中，两个非B帧之间允许连续出现的最大双向预测B帧数量
    pub has_b_frames: Option<u8>,

    /// 采样宽高比，单个像素自身的宽高比例
    pub sample_aspect_ratio: Option<String>,

    /// 显示宽高比，最终播放画面整体的宽高比例
    ///
    /// 计算公式：DAR = (画面宽度 × 像素宽) : (画面高度 × 像素高)
    pub display_aspect_ratio: Option<String>,

    /// 像素格式，定义视频帧的色彩编码、采样方式与内存存储排布
    pub pix_fmt: Option<String>,

    /// 编码等级，限定该编码配置支持的最大参数（分辨率、帧率、码率等）；实际等级为此数值除以10
    pub level: Option<u8>,

    /// 色度分量相对亮度分量的采样坐标位置
    pub chroma_location: Option<String>,

    /// 场序，定义视频的扫描方式
    pub field_order: Option<String>,

    /// 是否采用AVC封装格式
    pub is_avc: Option<String>,

    /// AVC封装中，每个NAL单元前置长度字段占用的字节数
    pub nal_length_size: Option<String>,

    /// 流在封装容器内的唯一标识，即容器层分配的流编号；视频流ID通常为`0x1`，音频流ID为`0x2`
    pub id: String,

    /// 标称/实时帧率：流预设帧率，使用分数存储保证精度
    pub r_frame_rate: String,

    /// 实际平均帧率，总帧数除以总时长计算得出，反映真实播放帧率
    pub avg_frame_rate: String,

    /// 时间基，当前流所有时间戳的最小计量单位；一个时间戳单位对应固定秒数
    pub time_base: String,

    /// 流首帧的显示时间戳(PTS)，单位为`time_base`
    pub start_pts: u16,

    /// 可读起始时间，等于 `start_pts` × `time_base`
    pub start_time: String,

    /// 流总时长对应的总时间戳数值，单位为 `time_base`
    pub duration_ts: u64,

    /// 总播放时长，等于 `duration_ts` × `time_base`
    pub duration: String,

    /// 流码率，每秒传输/存储的数据量，单位比特每秒(bps)
    pub bit_rate: String,

    /// 原始采样位深，每个色彩分量采样占用的比特位数
    pub bits_per_raw_sample: Option<String>,

    /// 流总帧数
    pub nb_frames: String,

    /// 附加私有数据大小，存储解码器初始化所需的专属配置参数，单位字节
    pub extradata_size: u8,

    /// 音频采样格式：描述单个采样点的数据类型与存储方式
    pub sample_fmt: Option<String>,

    /// 音频采样率，每秒对声音信号的采样次数
    pub sample_rate: Option<String>,

    /// 音频声道数量
    pub channels: Option<u8>,

    /// 声道布局，定义声道数量与各声道空间位置
    pub channel_layout: Option<String>,

    /// 此字段仅对无压缩PCM音频有效；AAC等压缩格式该值固定为0，不代表实际采样位深
    pub bits_per_sample: Option<u8>,

    /// 编码器初始化预留采样数
    pub initial_padding: Option<u8>,

    pub disposition: FfprobeDisposition,

    /// 流级元数据标签，结构随文件封装格式变化
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

/// 流属性标记集合
///
/// 结构体全局固定：所有文件、所有流的键名完全一致，值为0/1布尔整型
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, Serialize)]
pub struct FfprobeDisposition {
    #[serde(default)]
    pub default: u8,
    #[serde(default)]
    pub dub: u8,
    #[serde(default)]
    pub original: u8,
    #[serde(default)]
    pub comment: u8,
    #[serde(default)]
    pub lyrics: u8,
    #[serde(default)]
    pub karaoke: u8,
    #[serde(default)]
    pub forced: u8,
    #[serde(default)]
    pub hearing_impaired: u8,
    #[serde(default)]
    pub visual_impaired: u8,
    #[serde(default)]
    pub clean_effects: u8,
    #[serde(default)]
    pub attached_pic: u8,
    #[serde(default)]
    pub timed_thumbnails: u8,
    #[serde(default)]
    pub non_diegetic: u8,
    #[serde(default)]
    pub captions: u8,
    #[serde(default)]
    pub descriptions: u8,
    #[serde(default)]
    pub metadata: u8,
    #[serde(default)]
    pub dependent: u8,
    #[serde(default)]
    pub still_image: u8,
    #[serde(default)]
    pub multilayer: u8,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, Serialize)]
pub struct Format {
    /// 媒体文件完整路径与文件名
    pub filename: String,

    /// 文件包含的媒体流总数量
    pub nb_streams: u8,

    /// 文件包含的节目数量；节目概念主要用于MPEG-TS直播/广播传输流，可将多路音视频流封装至同一节目；MP4等常规本地文件无节目结构，该值为0
    pub nb_programs: u8,

    /// 文件包含的流分组数量；流分组多用于多视角视频、多音轨等特殊场景；标准单版本视频文件该值为0
    pub nb_stream_groups: u8,

    /// FFmpeg解封装器简称；逗号分隔列表代表该解封装器支持的全部封装格式
    #[serde(rename(deserialize = "format_name"), rename(serialize = "format_name"))]
    pub name: String,

    /// 封装格式完整官方名称
    #[serde(
        rename(deserialize = "format_long_name"),
        rename(serialize = "format_long_name")
    )]
    pub long_name: String,

    /// 整个文件的起始时间，取所有流中最早的起始时间
    pub start_time: String,

    /// 文件总播放时长，取所有流中最长时长作为封装容器总时长
    pub duration: String,

    /// 媒体文件总大小，单位字节
    pub size: String,

    /// 文件整体平均码率，文件总大小除以总时长计算，包含音视频数据与封装包头开销，单位bps
    pub bit_rate: String,

    /// 格式探测置信分，FFmpeg用于判定识别出的文件格式可靠程度的指标
    pub probe_score: u8,

    /// 文件元数据标签，结构随封装格式变化
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn sample_ffprobe_format() -> Format {
        Format {
            filename: "test.mp4".into(),
            nb_streams: 2,
            nb_programs: 0,
            nb_stream_groups: 0,
            name: "mov,mp4,m4a,3gp,3g2,mj2".into(),
            long_name: "QuickTime / MOV".into(),
            start_time: "0.000000".into(),
            duration: "10.500000".into(),
            size: "2097152".into(),
            bit_rate: "1597000".into(),
            probe_score: 100,
            tags: HashMap::default(),
        }
    }

    pub fn sample_ffprobe_video_stream() -> Stream {
        Stream {
            index: 0,
            codec_name: "h264".into(),
            codec_long_name: "H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10".into(),
            profile: "High".into(),
            codec_type: "video".into(),
            codec_tag_string: "avc1".into(),
            codec_tag: "0x31637661".into(),
            mime_codec_string: "avc1.640028".into(),
            width: Some(1920),
            height: Some(1080),
            coded_width: Some(1920),
            coded_height: Some(1088),
            has_b_frames: Some(2),
            sample_aspect_ratio: Some("1:1".into()),
            display_aspect_ratio: Some("16:9".into()),
            pix_fmt: Some("yuv420p".into()),
            level: Some(40),
            chroma_location: Some("left".into()),
            field_order: Some("progressive".into()),
            is_avc: Some("true".into()),
            nal_length_size: Some("4".into()),
            id: "0x1".into(),
            r_frame_rate: "25/1".into(),
            avg_frame_rate: "25/1".into(),
            time_base: "1/12800".into(),
            start_pts: 0,
            start_time: "0.000000".into(),
            duration_ts: 134_400,
            duration: "10.500000".into(),
            bit_rate: "1500000".into(),
            bits_per_raw_sample: Some("8".into()),
            nb_frames: "262".into(),
            extradata_size: 45,
            sample_fmt: None,
            sample_rate: None,
            channels: None,
            channel_layout: None,
            bits_per_sample: None,
            initial_padding: None,
            disposition: FfprobeDisposition {
                default: 1,
                ..Default::default()
            },
            tags: HashMap::default(),
        }
    }

    /// 构造标准测试用音频流原始结构
    pub fn sample_ffprobe_audio_stream() -> Stream {
        Stream {
            index: 1,
            codec_name: "aac".into(),
            codec_long_name: "AAC (Advanced Audio Coding)".into(),
            profile: "LC".into(),
            codec_type: "audio".into(),
            codec_tag_string: "mp4a".into(),
            codec_tag: "0x6134706d".into(),
            mime_codec_string: "mp4a.40.2".into(),
            width: None,
            height: None,
            coded_width: None,
            coded_height: None,
            has_b_frames: None,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            pix_fmt: None,
            level: None,
            chroma_location: None,
            field_order: None,
            is_avc: None,
            nal_length_size: None,
            id: "0x2".into(),
            r_frame_rate: "0/0".into(),
            avg_frame_rate: "0/0".into(),
            time_base: "1/44100".into(),
            start_pts: 0,
            start_time: "0.000000".into(),
            duration_ts: 463_872,
            duration: "10.518639".into(),
            bit_rate: "96000".into(),
            bits_per_raw_sample: None,
            nb_frames: "454".into(),
            extradata_size: 2,
            sample_fmt: Some("fltp".into()),
            sample_rate: Some("44100".into()),
            channels: Some(2),
            channel_layout: Some("stereo".into()),
            bits_per_sample: Some(16),
            initial_padding: Some(0),
            disposition: FfprobeDisposition {
                default: 1,
                ..Default::default()
            },
            tags: HashMap::default(),
        }
    }

    /// 构造字幕流（用于测试未知流过滤）
    pub fn sample_ffprobe_subtitle_stream() -> Stream {
        let mut stream = sample_ffprobe_video_stream();
        stream.index = 2;
        stream.codec_type = "subtitle".into();
        stream
    }

    /// 构造完整的标准 ffprobe 原始 JSON 结构
    pub fn sample_ffprobe_raw_json() -> FfprobeRawJson {
        FfprobeRawJson {
            streams: vec![sample_ffprobe_video_stream(), sample_ffprobe_audio_stream()],
            format: sample_ffprobe_format(),
        }
    }

    /// 构造标准 ffprobe JSON 字节数组（用于 fetcher 集成测试）
    pub fn sample_ffprobe_raw_json_bytes() -> Vec<u8> {
        serde_json::to_vec(&sample_ffprobe_raw_json()).unwrap()
    }
}
