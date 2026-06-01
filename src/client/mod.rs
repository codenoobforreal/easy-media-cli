mod ffmpeg_client;

use crate::{metadata::Metadata, progress::RawProgress};
use anyhow::Result;
use std::path::Path;

pub use ffmpeg_client::FfmpegClient;

pub trait Client: Send {
    fn metadata(&self, input: &Path) -> Result<Metadata>;

    fn generate_thumbnail_with_progress(
        &self,
        input: &Path,
        output: &Path,
        scene_threshold: f32,
        width: Option<u16>,
        progress_cb: &mut (dyn FnMut(RawProgress) + Send),
    ) -> Result<()>;
}
