use super::{Format, Stream};
use crate::{
    common::parse_float_str,
    domain::media::{AudioStream, MediaMetadata, MetadataFormat, VideoStream},
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

impl TryFrom<Format> for MetadataFormat {
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
