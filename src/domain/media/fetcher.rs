use crate::domain::Metadata;
use anyhow::Result;
use std::path::Path;

pub trait Fetcher: Send + Sync {
    fn fetch_metadata(&self, input: &Path) -> Result<Metadata>;
}
