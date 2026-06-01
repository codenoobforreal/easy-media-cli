use crate::{
    client::Client,
    progress::RawProgress,
    task::{Progress, Task, Type},
};
use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    time::Instant,
};

pub struct SceneCutSnapTask {
    input: PathBuf,
    output: PathBuf,
    threshold: f32,
    width: Option<u16>,
}

impl SceneCutSnapTask {
    pub fn new(input: PathBuf, output: PathBuf, threshold: f32, width: Option<u16>) -> Self {
        Self {
            input,
            output,
            threshold,
            width,
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        self.input.file_name().and_then(|s| s.to_str())
    }

    pub fn input(&self) -> &Path {
        &self.input
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    pub fn width(&self) -> Option<u16> {
        self.width
    }
}

impl Task for SceneCutSnapTask {
    fn task_type(&self) -> Type {
        Type::SceneCutSnap
    }

    fn supports_progress(&self) -> bool {
        true
    }

    fn execute(&self, _: Box<dyn Client>) -> Result<()> {
        unreachable!()
    }

    fn execute_with_progress(
        &self,
        client: Box<dyn Client>,
        progress_sender: Sender<Progress>,
    ) -> Result<()> {
        let start_time = Instant::now();
        let metadata = client.metadata(self.input())?;
        let duration = metadata.duration();

        let mut last_progress = Progress::default();
        let mut progress_callback = move |raw_progress: RawProgress| {
            let elapsed = start_time.elapsed();
            let progress = Progress::from_raw_progress(raw_progress, duration, elapsed);
            if progress.should_update(&last_progress) {
                last_progress = progress;
                let _ = progress_sender.send(progress);
            }
        };

        client.generate_thumbnail_with_progress(
            self.input(),
            self.output(),
            self.threshold,
            self.width,
            &mut progress_callback,
        )
    }
}
