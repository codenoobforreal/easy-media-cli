use crate::{
    domain::TaskResultPayload,
    infra::{FfprobeRawJson, convert_raw_to_metadata},
    task::{ExecutionMode, FfmpegTask},
    tasks::{LOG_ERROR_ARGS, OUTPUT_FORMAT_JSON_ARGS, SHOW_ENTRIES_ARGS},
};
use anyhow::{Result, anyhow};
use serde_json::from_slice;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct MediaMetadataGetter {
    id: usize,
    name: String,
    input: PathBuf,
}

impl MediaMetadataGetter {
    pub fn new(id: usize, input: PathBuf) -> Self {
        let name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .map_or("Retrive metadata".to_string(), |s| {
                format!("Retrive metadata: {s}")
            });

        Self { id, name, input }
    }
}

impl FfmpegTask for MediaMetadataGetter {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn input(&self) -> &Path {
        &self.input
    }

    fn output(&self) -> Option<&Path> {
        None
    }

    fn build_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        args.extend(LOG_ERROR_ARGS.iter().map(OsString::from));
        args.extend(SHOW_ENTRIES_ARGS.iter().map(OsString::from));
        args.extend(OUTPUT_FORMAT_JSON_ARGS.iter().map(OsString::from));
        args.push(OsString::from(&self.input));
        args
    }

    fn file_name(&self) -> Option<&OsStr> {
        self.input.file_name()
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Capturing
    }

    fn needs_progress(&self) -> bool {
        false
    }

    fn handle_captured_output(
        &self,
        stdout: &[u8],
        _stderr: &[u8],
    ) -> Result<Option<TaskResultPayload>> {
        let probe_result: FfprobeRawJson = from_slice(stdout)
            .map_err(|e| anyhow!("Failed to deserialize ffprobe JSON output: {e}"))?;
        let metadata = convert_raw_to_metadata(probe_result)?;

        Ok(Some(TaskResultPayload::MediaMetadataGetter { metadata }))
    }
}
