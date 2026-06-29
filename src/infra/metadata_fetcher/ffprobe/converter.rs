use super::{Format, Stream};
use crate::{
    common::parse_float_str,
    domain::media::{AudioStream, MediaMetadata, MetadataFormat, VideoStream},
    infra::FfprobeRawJson,
};
use std::time::Duration;

pub fn convert_raw_to_metadata(probe: FfprobeRawJson) -> MediaMetadata {
    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();

    for stream in probe.streams {
        match stream.codec_type.as_str() {
            "video" => {
                video_streams.push(stream.into());
            }

            "audio" => {
                audio_streams.push(stream.into());
            }

            _ => {}
        }
    }

    MediaMetadata::new(probe.format.into(), video_streams, audio_streams)
}

impl From<Stream> for VideoStream {
    fn from(stream: Stream) -> Self {
        Self {
            codec_name: stream.codec_name,
            codec_long_name: stream.codec_long_name,
            codec_tag_string: stream.codec_tag_string,
            width: stream.width,
            height: stream.height,
            pix_fmt: stream.pix_fmt,
            is_avc: stream.is_avc.is_some_and(|x| x == "true"),
            r_frame_rate: parse_float_str(stream.r_frame_rate),
            avg_frame_rate: parse_float_str(stream.avg_frame_rate),
            start_time: stream
                .start_time
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            duration: stream
                .duration
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            bit_rate: stream.bit_rate.and_then(|b| b.parse().ok()),
            nb_frames: stream.nb_frames.parse().ok(),
            is_default: stream.disposition.default == 1,
        }
    }
}

impl From<Stream> for AudioStream {
    fn from(stream: Stream) -> Self {
        Self {
            codec_name: stream.codec_name,
            codec_long_name: stream.codec_long_name,
            codec_tag_string: stream.codec_tag_string,
            sample_rate: stream.sample_rate.and_then(|s| s.parse().ok()),
            channels: stream.channels,
            channel_layout: stream.channel_layout,
            r_frame_rate: parse_float_str(stream.r_frame_rate),
            avg_frame_rate: parse_float_str(stream.avg_frame_rate),
            start_time: stream
                .start_time
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            duration: stream
                .duration
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            bit_rate: stream.bit_rate.and_then(|b| b.parse().ok()),
            nb_frames: stream.nb_frames.parse().ok(),
            is_default: stream.disposition.default == 1,
        }
    }
}

impl From<Format> for MetadataFormat {
    fn from(format: Format) -> Self {
        Self {
            filename: format.filename,
            name: format.name,
            long_name: format.long_name,
            start_time: format
                .start_time
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            duration: format
                .duration
                .parse::<f64>()
                .ok()
                .map(Duration::from_secs_f64),
            size: format.size.parse().ok(),
            bit_rate: format.bit_rate.parse().ok(),
        }
    }
}
