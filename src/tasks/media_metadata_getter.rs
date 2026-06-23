use crate::{
    common::format_duration,
    domain::Event,
    infra::{EventBus, FfprobeRawJson, convert_raw_to_metadata},
    task::{ExecutionMode, FfmpegTask},
};
use anyhow::{Result, anyhow};
use serde_json::from_slice;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct MediaMetadataGetter {
    id: usize,
    input: PathBuf,
    event_bus: Arc<dyn EventBus>,
}

impl MediaMetadataGetter {
    pub fn new(id: usize, input: PathBuf, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            id,
            input,
            event_bus,
        }
    }
}

impl FfmpegTask for MediaMetadataGetter {
    fn id(&self) -> usize {
        self.id
    }

    fn name(&self) -> Option<&str> {
        self.input.file_stem().and_then(|s| s.to_str())
    }

    fn input(&self) -> &Path {
        &self.input
    }

    fn output(&self) -> Option<&Path> {
        None
    }

    fn build_args(&self) -> Vec<OsString> {
        vec![
            OsString::from("-v"),
            OsString::from("quiet"),
            OsString::from("-show_entries"),
            OsString::from("program:format:stream:chapter"),
            OsString::from("-of"),
            OsString::from("json"),
            OsString::from(&self.input),
        ]
    }

    fn file_name(&self) -> Option<&std::ffi::OsStr> {
        self.input.file_name()
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Capturing
    }

    fn needs_progress(&self) -> bool {
        false
    }

    fn handle_captured_output(&self, stdout: &[u8], _stderr: &[u8]) -> Result<()> {
        let probe_result: FfprobeRawJson = from_slice(stdout)
            .map_err(|e| anyhow!("Failed to deserialize ffprobe JSON output: {e}"))?;
        let metadata = convert_raw_to_metadata(probe_result)?;

        let summary = format!(
            "时长: {} | 分辨率: {}x{}",
            format_duration(metadata.duration()),
            metadata.width().unwrap_or_default(),
            metadata.height().unwrap_or_default()
        );
        self.event_bus.publish(Event::TaskResult {
            id: self.id,
            summary,
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::test_utils::{MockEventBus, sample_ffprobe_raw_json_bytes};
    use insta::assert_debug_snapshot;

    #[test]
    fn execution_mode_is_capturing() {
        let bus = Arc::new(MockEventBus::default());
        let task = MediaMetadataGetter::new(1, "test.mp4".into(), bus);
        assert_eq!(task.execution_mode(), ExecutionMode::Capturing);
    }

    #[test]
    fn needs_progress_is_false() {
        let bus = Arc::new(MockEventBus::default());
        let task = MediaMetadataGetter::new(1, "test.mp4".into(), bus);
        assert!(!task.needs_progress());
    }

    #[test]
    fn output_returns_none() {
        let bus = Arc::new(MockEventBus::default());
        let task = MediaMetadataGetter::new(1, "test.mp4".into(), bus);
        assert!(task.output().is_none());
    }

    mod handle_captured_output {
        use super::*;

        #[test]
        fn valid_json_publishes_result_event() {
            let bus = Arc::new(MockEventBus::default());
            let task = MediaMetadataGetter::new(1, "test.mp4".into(), bus.clone());
            task.handle_captured_output(&sample_ffprobe_raw_json_bytes(), &[])
                .unwrap();
            let events = bus.events();
            assert_eq!(events.len(), 1);
            assert_debug_snapshot!(&events[0],@r#"
            TaskResult {
                id: 1,
                summary: "时长: 00:00:10 | 分辨率: 1920x1080",
            }
            "#);
        }

        #[test]
        fn invalid_json_returns_deserialize_error() {
            let bus = Arc::new(MockEventBus::default());
            let task = MediaMetadataGetter::new(1, "test.mp4".into(), bus);
            let err = task.handle_captured_output(b"not json", &[]).unwrap_err();
            assert_debug_snapshot!(err,@r#""Failed to deserialize ffprobe JSON output: expected ident at line 1 column 2""#);
        }
    }

    #[test]
    fn build_args_contains_ffprobe_standard_flags() {
        let bus = Arc::new(MockEventBus::default());
        let task = MediaMetadataGetter::new(1, "input.mp4".into(), bus);
        let args: Vec<String> = task
            .build_args()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_debug_snapshot!(args,@r#"
        [
            "-v",
            "quiet",
            "-show_entries",
            "program:format:stream:chapter",
            "-of",
            "json",
            "input.mp4",
        ]
        "#);
    }
}
