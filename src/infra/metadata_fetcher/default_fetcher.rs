use crate::{
    domain::{Fetcher as MetadataFetcher, Metadata as MediaMetadata},
    infra::{
        CapturingCommandRunner, CapturingCommandRunnerExt, FfprobeRawJson, convert_raw_to_metadata,
    },
};
use anyhow::Result;
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

        let raw: FfprobeRawJson = from_slice(&output.stdout)?;

        convert_raw_to_metadata(raw)
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use anyhow::anyhow;
    use std::{path::PathBuf, sync::Mutex};

    #[derive(Default)]
    pub struct MockMetadataFetcher {
        state: Mutex<FetcherState>,
    }

    #[derive(Default)]
    struct FetcherState {
        ok: Option<MediaMetadata>,
        err: Option<String>,
        calls: Vec<PathBuf>,
    }

    impl MockMetadataFetcher {
        pub fn set_ok(&self, metadata: MediaMetadata) {
            let mut s = self.state.lock().unwrap();
            s.ok = Some(metadata);
            s.err = None;
        }

        pub fn set_err(&self, msg: &'static str) {
            let mut s = self.state.lock().unwrap();
            s.err = Some(msg.to_string());
            s.ok = None;
        }

        pub fn call_count(&self) -> usize {
            self.state.lock().unwrap().calls.len()
        }

        pub fn last_call_path(&self) -> Option<PathBuf> {
            self.state.lock().unwrap().calls.last().cloned()
        }
    }

    impl MetadataFetcher for MockMetadataFetcher {
        fn fetch_metadata(&self, path: &Path) -> Result<MediaMetadata> {
            let mut s = self.state.lock().unwrap();
            s.calls.push(path.to_path_buf());
            if let Some(msg) = &s.err {
                return Err(anyhow!(msg.clone()));
            }
            s.ok.clone()
                .ok_or_else(|| anyhow!("No metadata result configured"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::test_utils::{
        MockCommandRunner, MockMetadataFetcher, exit_status, sample_ffprobe_raw_json_bytes,
    };
    use insta::assert_debug_snapshot;
    use std::time::Duration;

    #[test]
    fn mock_fetcher_returns_configured_success() {
        let fetcher = MockMetadataFetcher::default();
        let meta = MediaMetadata::default();
        fetcher.set_ok(meta.clone());
        let result = fetcher.fetch_metadata(Path::new("/test.mp4")).unwrap();
        assert_eq!(result.video_streams.len(), 0);
    }

    #[test]
    fn mock_fetcher_returns_configured_error() {
        let fetcher = MockMetadataFetcher::default();
        fetcher.set_err("Probe failed");
        let err = fetcher.fetch_metadata(Path::new("/test.mp4")).unwrap_err();
        assert_debug_snapshot!(err,@r#""Probe failed""#);
    }

    #[test]
    fn mock_fetcher_no_preset_returns_default_error() {
        let fetcher = MockMetadataFetcher::default();
        let err = fetcher.fetch_metadata(Path::new("/test.mp4")).unwrap_err();
        assert_debug_snapshot!(err,@r#""No metadata result configured""#);
    }

    #[test]
    fn default_fetcher_parses_ffprobe_output_correctly() {
        let runner = Arc::new(MockCommandRunner::default());
        runner.set_capture_ok(exit_status(true), sample_ffprobe_raw_json_bytes(), vec![]);
        let fetcher = DefaultMetadataFetcher::new(runner);
        let meta = fetcher
            .fetch_metadata(Path::new("/input/test.mp4"))
            .unwrap();
        assert_eq!(meta.video_streams.len(), 1);
        assert_eq!(meta.audio_streams.len(), 1);
        assert_eq!(meta.width(), Some(1920));
        assert_eq!(meta.duration(), Duration::from_secs_f64(10.5));
    }

    #[test]
    fn default_fetcher_propagates_runner_error() {
        let runner = Arc::new(MockCommandRunner::default());
        runner.set_capture_err("ffprobe not found");
        let fetcher = DefaultMetadataFetcher::new(runner);
        let err = fetcher.fetch_metadata(Path::new("/test.mp4")).unwrap_err();
        assert_debug_snapshot!(err,@r#""ffprobe not found""#);
    }

    #[test]
    fn default_fetcher_invalid_json_returns_error() {
        let runner = Arc::new(MockCommandRunner::default());
        runner.set_capture_ok(exit_status(true), b"not valid json".to_vec(), vec![]);
        let fetcher = DefaultMetadataFetcher::new(runner);
        let err = fetcher.fetch_metadata(Path::new("/test.mp4")).unwrap_err();
        assert_debug_snapshot!(err,@r#"Error("expected ident", line: 1, column: 2)"#);
    }
}
