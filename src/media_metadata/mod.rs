//! 领域服务：媒体元数据获取

mod fetcher;
mod ffprobe;
mod metadata;

use anyhow::Result;
#[cfg(test)]
pub use fetcher::tests::MockMetadataFetcher;
pub use fetcher::{DefaultMetadataFetcher, MetadataFetcher};
pub use ffprobe::raw_json::FfprobeRawJson;
pub use metadata::MediaMetadata;

#[cfg(test)]
pub use ffprobe::{
    sample_ffprobe_audio_stream, sample_ffprobe_format, sample_ffprobe_raw_json,
    sample_ffprobe_raw_json_bytes, sample_ffprobe_subtitle_stream, sample_ffprobe_video_stream,
};
#[cfg(test)]
pub use metadata::Format;

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

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;
    use std::time::Duration;

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
