use crate::domain::media::MediaMetadata;
use anyhow::Result;
use std::path::Path;

pub trait MetadataFetcher: Send + Sync {
    fn fetch_metadata(&self, input: &Path) -> Result<MediaMetadata>;
}
