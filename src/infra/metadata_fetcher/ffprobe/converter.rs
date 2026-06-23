use super::{Format, Stream};
use crate::{
    common::parse_float_str,
    domain::{AudioStream, Format as MediaMetadataFormat, Metadata as MediaMetadata, VideoStream},
    infra::FfprobeRawJson,
};
use anyhow::{Context, Result};
use std::time::Duration;

pub fn convert_raw_to_metadata(probe: FfprobeRawJson) -> Result<MediaMetadata> {
    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();

    for stream in probe.streams {
        match stream.codec_type.as_str() {
            "video" => {
                video_streams.push(stream.try_into()?);
            }

            "audio" => {
                audio_streams.push(stream.try_into()?);
            }

            _ => {}
        }
    }

    Ok(MediaMetadata::new(
        probe.format.try_into()?,
        video_streams,
        audio_streams,
    ))
}

impl TryFrom<Stream> for VideoStream {
    type Error = anyhow::Error;

    fn try_from(stream: Stream) -> Result<Self, Self::Error> {
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

impl TryFrom<Stream> for AudioStream {
    type Error = anyhow::Error;

    fn try_from(stream: Stream) -> Result<Self, Self::Error> {
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

impl TryFrom<Format> for MediaMetadataFormat {
    type Error = anyhow::Error;

    fn try_from(format: Format) -> Result<Self, Self::Error> {
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
    use crate::infra::test_utils::{
        sample_ffprobe_audio_stream, sample_ffprobe_format, sample_ffprobe_raw_json,
        sample_ffprobe_subtitle_stream, sample_ffprobe_video_stream,
    };
    use insta::assert_debug_snapshot;
    use std::time::Duration;

    mod convert_raw_to_metadata {
        use super::*;

        #[test]
        fn normal_file_classifies_streams_correctly() {
            let raw = sample_ffprobe_raw_json();
            let meta = convert_raw_to_metadata(raw).unwrap();
            assert_eq!(meta.video_streams.len(), 1);
            assert_eq!(meta.audio_streams.len(), 1);
            assert_eq!(meta.format.duration, Duration::from_secs_f64(10.5));
        }

        #[test]
        fn ignores_non_video_audio_streams() {
            let mut raw = sample_ffprobe_raw_json();
            raw.streams.push(sample_ffprobe_subtitle_stream());
            let meta = convert_raw_to_metadata(raw).unwrap();
            assert_eq!(meta.video_streams.len(), 1);
            assert_eq!(meta.audio_streams.len(), 1);
        }

        #[test]
        fn no_video_streams_returns_empty_vec() {
            let raw = FfprobeRawJson {
                streams: vec![sample_ffprobe_audio_stream()],
                format: sample_ffprobe_format(),
            };
            let meta = convert_raw_to_metadata(raw).unwrap();
            assert!(meta.video_streams.is_empty());
            assert_eq!(meta.audio_streams.len(), 1);
        }

        #[test]
        fn no_audio_streams_returns_empty_vec() {
            let raw = FfprobeRawJson {
                streams: vec![sample_ffprobe_video_stream()],
                format: sample_ffprobe_format(),
            };
            let meta = convert_raw_to_metadata(raw).unwrap();
            assert_eq!(meta.video_streams.len(), 1);
            assert!(meta.audio_streams.is_empty());
        }

        #[test]
        fn empty_streams_returns_empty_lists() {
            let raw = FfprobeRawJson {
                streams: vec![],
                format: sample_ffprobe_format(),
            };
            let meta = convert_raw_to_metadata(raw).unwrap();
            assert!(meta.video_streams.is_empty());
            assert!(meta.audio_streams.is_empty());
        }

        #[test]
        fn invalid_stream_propagates_error() {
            let mut raw = sample_ffprobe_raw_json();
            raw.streams[0].width = None; // 破坏视频流必填字段
            let err = convert_raw_to_metadata(raw).unwrap_err();
            assert_debug_snapshot!(err,@"Missing width field from ffprobe json output");
        }
    }

    mod tryfrom {
        use super::*;

        #[test]
        fn normal_format_converts_correctly() {
            let raw = sample_ffprobe_format();
            let format: MediaMetadataFormat = raw.try_into().unwrap();
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
            let err = MediaMetadataFormat::try_from(raw).unwrap_err();
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
            let err = MediaMetadataFormat::try_from(raw).unwrap_err();
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
            assert_debug_snapshot!(stream,@r#"
            VideoStream {
                codec_name: "h264",
                codec_long_name: "H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10",
                codec_tag_string: "avc1",
                width: 1920,
                height: 1080,
                pix_fmt: "yuv420p",
                is_avc: true,
                r_frame_rate: Some(
                    25.0,
                ),
                avg_frame_rate: Some(
                    25.0,
                ),
                start_time: 0ns,
                duration: 10.5s,
                bit_rate: 1500000,
                nb_frames: 262,
                is_default: true,
            }
            "#);
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
}
