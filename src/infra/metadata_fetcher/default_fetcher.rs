use crate::{
    domain::media::{MediaMetadata, MetadataFetcher},
    infra::{
        CapturingCommandRunner, CapturingCommandRunnerExt, FfprobeRawJson, convert_raw_to_metadata,
    },
};
use anyhow::{Context, Result};
use serde_json::from_slice;
use std::{path::Path, sync::Arc};

pub struct DefaultMetadataFetcher {
    runner: Arc<dyn CapturingCommandRunner>,
}

impl DefaultMetadataFetcher {
    pub fn new(runner: Arc<dyn CapturingCommandRunner>) -> Self {
        Self { runner }
    }
}

impl MetadataFetcher for DefaultMetadataFetcher {
    fn fetch_metadata(&self, input: &Path) -> Result<MediaMetadata> {
        let output = self.runner.run_and_capture(
            "ffprobe",
            &[
                "-v".as_ref(),
                "error".as_ref(),
                "-show_entries".as_ref(),
                "program:format:stream:chapter".as_ref(),
                "-of".as_ref(),
                "json".as_ref(),
                input.as_os_str(),
            ],
        )?;

        let raw: FfprobeRawJson = from_slice(&output.stdout).with_context(|| {
            format!("Failed to retrive metadata for input: {}", input.display())
        })?;

        Ok(convert_raw_to_metadata(raw))
    }
}
