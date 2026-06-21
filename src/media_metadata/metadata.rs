//! 业务级结构化元数据实体，类型转换已完成

use crate::{
    common::parse_float_str,
    media_metadata::ffprobe::raw_json::{Format as FfprobeFormat, Stream as FfprobeStream},
};
use anyhow::{Context, Ok};
use std::time::Duration;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct MediaMetadata {
    pub format: Format,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    // pub format_tags: HashMap<String, String>,
}

/// TODO 获取到的数据可能是不准确的，现在默认选取每种流的第一个流作为结果返回
impl MediaMetadata {
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
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct VideoStream {
    pub codec_name: String,
    pub codec_long_name: String,
    pub codec_tag_string: String,
    pub width: u16,
    pub height: u16,
    // pub coded_width: u16,
    // pub coded_height: u16,
    pub pix_fmt: String,
    pub is_avc: bool,
    pub r_frame_rate: Option<f64>,
    pub avg_frame_rate: Option<f64>,
    pub start_time: Duration,
    pub duration: Duration,
    pub bit_rate: u64,
    pub nb_frames: u32,
    /// Determined by the `default` field in the disposition; 1 means true, 0 means false
    pub is_default: bool,
    // pub tags: HashMap<String, String>,
}

impl TryFrom<FfprobeStream> for VideoStream {
    type Error = anyhow::Error;

    fn try_from(stream: FfprobeStream) -> Result<Self, Self::Error> {
        Ok(Self {
            codec_name: stream.codec_name,
            codec_long_name: stream.codec_long_name,
            codec_tag_string: stream.codec_tag_string,
            width: stream
                .width
                .with_context(|| "Missing width field from ffprobe json output".to_owned())?,
            height: stream
                .height
                .with_context(|| "Missing height field from ffprobe json output".to_owned())?,
            pix_fmt: stream
                .pix_fmt
                .with_context(|| "Missing pix_fmt field from ffprobe json output".to_owned())?,
            is_avc: stream.is_avc.is_some_and(|x| x == "true"),
            r_frame_rate: parse_float_str(stream.r_frame_rate),
            avg_frame_rate: parse_float_str(stream.avg_frame_rate),
            start_time: Duration::from_secs_f64(stream.start_time.parse::<f64>()?),
            duration: Duration::from_secs_f64(stream.duration.parse::<f64>()?),
            bit_rate: stream.bit_rate.parse()?,
            nb_frames: stream.nb_frames.parse()?,
            is_default: stream.disposition.default == 1,
        })
    }
}

#[allow(dead_code)]
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
    /// Determined by the `default` field in the disposition; 1 means true, 0 means false
    pub is_default: bool,
    // pub tags: HashMap<String, String>,
}

impl TryFrom<FfprobeStream> for AudioStream {
    type Error = anyhow::Error;

    fn try_from(stream: FfprobeStream) -> Result<Self, Self::Error> {
        Ok(Self {
            codec_name: stream.codec_name,
            codec_long_name: stream.codec_long_name,
            codec_tag_string: stream.codec_tag_string,
            sample_rate: stream
                .sample_rate
                .with_context(|| "Missing sample_rate field from ffprobe json output".to_owned())?
                .parse()?,
            channels: stream
                .channels
                .with_context(|| "Missing channels field from ffprobe json output".to_owned())?,
            channel_layout: stream.channel_layout.with_context(|| {
                "Missing channel_layout field from ffprobe json output".to_owned()
            })?,
            r_frame_rate: parse_float_str(stream.r_frame_rate),
            avg_frame_rate: parse_float_str(stream.avg_frame_rate),
            start_time: Duration::from_secs_f64(stream.start_time.parse::<f64>()?),
            duration: Duration::from_secs_f64(stream.duration.parse::<f64>()?),
            bit_rate: stream.bit_rate.parse()?,
            nb_frames: stream.nb_frames.parse()?,
            is_default: stream.disposition.default == 1,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct Format {
    pub filename: String,
    /// `json` 实际字段名为 `format_name`
    pub name: String,
    /// `json` 实际字段名为 `format_long_name`
    pub long_name: String,
    pub start_time: Duration,
    pub duration: Duration,
    pub size: u64,
    pub bit_rate: u64,
}

impl TryFrom<FfprobeFormat> for Format {
    type Error = anyhow::Error;

    fn try_from(format: FfprobeFormat) -> Result<Self, Self::Error> {
        Ok(Self {
            filename: format.filename,
            name: format.name,
            long_name: format.long_name,
            start_time: Duration::from_secs_f64(format.start_time.parse::<f64>()?),
            duration: Duration::from_secs_f64(format.duration.parse::<f64>()?),
            size: format.size.parse()?,
            bit_rate: format.bit_rate.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_metadata::{
        sample_ffprobe_audio_stream, sample_ffprobe_format, sample_ffprobe_video_stream,
    };
    use insta::assert_debug_snapshot;

    #[test]
    fn normal_format_converts_correctly() {
        let raw = sample_ffprobe_format();
        let format: Format = raw.try_into().unwrap();
        assert_debug_snapshot!(format,@r#"
        Format {
            filename: "test.mp4",
            name: "mov,mp4,m4a,3gp,3g2,mj2",
            long_name: "QuickTime / MOV",
            start_time: 0ns,
            duration: 10.5s,
            size: 2097152,
            bit_rate: 1597000,
        }
        "#);
    }

    #[test]
    fn invalid_duration_returns_error() {
        let mut raw = sample_ffprobe_format();
        raw.duration = "invalid_number".into();
        let err = Format::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"
        ParseFloatError {
            kind: Invalid,
        }
        ");
    }

    #[test]
    fn invalid_size_returns_error() {
        let mut raw = sample_ffprobe_format();
        raw.size = "not_a_number".into();
        let err = Format::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"
        ParseIntError {
            kind: InvalidDigit,
        }
        ");
    }

    #[test]
    fn normal_video_stream_converts_correctly() {
        let raw = sample_ffprobe_video_stream();
        let stream: VideoStream = raw.try_into().unwrap();
        assert_debug_snapshot!(stream,@"");
    }

    #[test]
    fn missing_width_returns_error() {
        let mut raw = sample_ffprobe_video_stream();
        raw.width = None;
        let err = VideoStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing width field from ffprobe json output");
    }

    #[test]
    fn missing_height_returns_error() {
        let mut raw = sample_ffprobe_video_stream();
        raw.height = None;
        let err = VideoStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing height field from ffprobe json output");
    }

    #[test]
    fn missing_pix_fmt_returns_error() {
        let mut raw = sample_ffprobe_video_stream();
        raw.pix_fmt = None;
        let err = VideoStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing pix_fmt field from ffprobe json output");
    }

    #[test]
    fn is_avc_false_when_not_true() {
        let mut raw = sample_ffprobe_video_stream();
        raw.is_avc = Some("false".into());
        let stream: VideoStream = raw.try_into().unwrap();
        assert!(!stream.is_avc);
    }

    #[test]
    fn is_avc_false_when_none() {
        let mut raw = sample_ffprobe_video_stream();
        raw.is_avc = None;
        let stream: VideoStream = raw.try_into().unwrap();
        assert!(!stream.is_avc);
    }

    #[test]
    fn zero_fraction_frame_rate_returns_none() {
        let mut raw = sample_ffprobe_video_stream();
        raw.r_frame_rate = "0/0".into();
        let stream: VideoStream = raw.try_into().unwrap();
        assert!(stream.r_frame_rate.is_none());
    }

    #[test]
    fn non_default_disposition_sets_flag_false() {
        let mut raw = sample_ffprobe_video_stream();
        raw.disposition.default = 0;
        let stream: VideoStream = raw.try_into().unwrap();
        assert!(!stream.is_default);
    }

    #[test]
    fn normal_audio_stream_converts_correctly() {
        let raw = sample_ffprobe_audio_stream();
        let stream: AudioStream = raw.try_into().unwrap();
        assert_debug_snapshot!(stream,@r#"
        AudioStream {
            codec_name: "aac",
            codec_long_name: "AAC (Advanced Audio Coding)",
            codec_tag_string: "mp4a",
            sample_rate: 44100,
            channels: 2,
            channel_layout: "stereo",
            r_frame_rate: None,
            avg_frame_rate: None,
            start_time: 0ns,
            duration: 10.518639s,
            bit_rate: 96000,
            nb_frames: 454,
            is_default: true,
        }
        "#);
    }

    #[test]
    fn missing_sample_rate_returns_error() {
        let mut raw = sample_ffprobe_audio_stream();
        raw.sample_rate = None;
        let err = AudioStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing sample_rate field from ffprobe json output");
    }

    #[test]
    fn missing_channels_returns_error() {
        let mut raw = sample_ffprobe_audio_stream();
        raw.channels = None;
        let err = AudioStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing channels field from ffprobe json output");
    }

    #[test]
    fn missing_channel_layout_returns_error() {
        let mut raw = sample_ffprobe_audio_stream();
        raw.channel_layout = None;
        let err = AudioStream::try_from(raw).unwrap_err();
        assert_debug_snapshot!(err,@"Missing channel_layout field from ffprobe json output");
    }
}
