//! 领域服务：媒体元数据获取

mod default_fetcher;
mod ffprobe;

pub use default_fetcher::DefaultMetadataFetcher;
pub use ffprobe::{FfprobeRawJson, convert_raw_to_metadata};
