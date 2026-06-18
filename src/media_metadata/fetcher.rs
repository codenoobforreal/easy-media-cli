use crate::{
    infra::{CapturingCommandRunner, CapturingCommandRunnerExt},
    media_metadata::{MediaMetadata, convert_raw_to_metadata, ffprobe::raw_json::FfprobeRawJson},
};
use anyhow::Result;
use serde_json::from_slice;
use std::{path::Path, sync::Arc};

pub trait MetadataFetcher: Send + Sync {
    fn fetch_metadata(&self, input: &Path) -> Result<MediaMetadata>;
}

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
pub mod tests {
    use super::*;
    use crate::{
        infra::{MockCommandRunner, exit_status},
        media_metadata::sample_ffprobe_raw_json_bytes,
    };
    use insta::assert_debug_snapshot;
    use std::{path::PathBuf, sync::Mutex, time::Duration};

    #[derive(Default)]
    pub struct MockMetadataFetcher {
        ok_result: Mutex<Option<MediaMetadata>>,
        err_msg: Mutex<Option<String>>,
        call_history: Mutex<Vec<PathBuf>>,
    }

    impl MockMetadataFetcher {
        /// 设置成功返回结果，配置一次可重复调用多次返回
        pub fn set_ok(&self, metadata: MediaMetadata) {
            *self.ok_result.lock().unwrap() = Some(metadata);
            *self.err_msg.lock().unwrap() = None;
        }

        /// 设置错误返回结果，配置一次可重复调用多次返回
        pub fn set_err(&self, msg: &'static str) {
            *self.err_msg.lock().unwrap() = Some(msg.to_string());
            *self.ok_result.lock().unwrap() = None;
        }

        /// 获取调用历史，校验是否按预期路径调用
        pub fn call_count(&self) -> usize {
            self.call_history.lock().unwrap().len()
        }

        pub fn last_call_path(&self) -> Option<PathBuf> {
            self.call_history.lock().unwrap().last().cloned()
        }
    }

    impl MetadataFetcher for MockMetadataFetcher {
        fn fetch_metadata(&self, path: &Path) -> Result<MediaMetadata> {
            // 记录调用路径
            self.call_history.lock().unwrap().push(path.to_path_buf());

            // 优先返回错误，错误可重复触发
            if let Some(msg) = self.err_msg.lock().unwrap().as_ref() {
                return Err(anyhow::anyhow!(msg.clone()));
            }

            // 成功结果克隆返回，不消耗原值，支持多次调用
            self.ok_result
                .lock()
                .unwrap()
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("No metadata result set"))
        }
    }

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
        assert_debug_snapshot!(err,@r#""No metadata result set""#);
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
