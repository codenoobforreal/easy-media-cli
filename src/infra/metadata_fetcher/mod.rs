//! 领域服务：媒体元数据获取

mod default_fetcher;
mod ffprobe;

pub use default_fetcher::DefaultMetadataFetcher;
pub use ffprobe::{FfprobeRawJson, convert_raw_to_metadata};

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use default_fetcher::test_utils::MockMetadataFetcher;
    pub use ffprobe::test_utils::{
        sample_ffprobe_audio_stream, sample_ffprobe_format, sample_ffprobe_raw_json,
        sample_ffprobe_raw_json_bytes, sample_ffprobe_subtitle_stream, sample_ffprobe_video_stream,
    };
}
