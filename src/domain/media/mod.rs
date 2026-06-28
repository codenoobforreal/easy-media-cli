//! 媒体子领域

mod fetcher;
mod metadata;
mod resolution;

pub use fetcher::MetadataFetcher;
pub use metadata::{AudioStream, MediaMetadata, MetadataFormat, VideoStream};
pub use resolution::{Orientation, Resolution, ResolutionError};
