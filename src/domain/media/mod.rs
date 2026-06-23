//! 媒体子领域

mod fetcher;
mod metadata;
mod resolution;

pub use fetcher::Fetcher;
pub use metadata::{AudioStream, Format, Metadata, VideoStream};
pub use resolution::{Orientation, Resolution, ResolutionError};
