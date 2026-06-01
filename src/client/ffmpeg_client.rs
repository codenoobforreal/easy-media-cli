use crate::{
    client::Client,
    command_executor::{CommandExecutor, DefaultCommandExecutor},
    metadata::{Metadata, parse_raw_metadata},
    progress::RawProgress,
};
use anyhow::{Result, anyhow};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const FFMPEG_QUIET: &[&str] = &["-v", "error"];
const FFMPEG_PROGRESS: &[&str] = &["-progress", "pipe:1"];
const FFMPEG_OVERWRITE: &[&str] = &["-y"];

const FFPROBE_FORMAT: &[&str] = &[
    "-show_entries",
    "stream:format",
    "-of",
    "default=noprint_wrappers=1",
];

pub struct FfmpegClient<'bin_path> {
    ffmpeg_path: &'bin_path Path,
    ffprobe_path: &'bin_path Path,
    executor: Box<dyn CommandExecutor>,
}

impl Default for FfmpegClient<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl FfmpegClient<'_> {
    pub fn new() -> Self {
        ClientBuilder::new().build()
    }

    pub fn builder() -> ClientBuilder<'static> {
        ClientBuilder::new()
    }

    pub fn with_executor(executor: Box<dyn CommandExecutor>) -> Self {
        ClientBuilder::new().executor(executor).build()
    }
}

impl Client for FfmpegClient<'_> {
    fn metadata(&self, input: &Path) -> Result<Metadata> {
        let mut cmd = self.build_metadata_command(input);
        let output = self.executor.execute(&mut cmd)?;
        parse_raw_metadata(&output)
    }

    fn generate_thumbnail_with_progress(
        &self,
        input: &Path,
        output: &Path,
        scene_threshold: f32,
        width: Option<u16>,
        progress_cb: &mut (dyn FnMut(RawProgress) + Send),
    ) -> Result<()> {
        let mut cmd = self.build_thumbnail_command(input, output, scene_threshold, width)?;
        self.executor.execute_with_progress(&mut cmd, progress_cb)
    }
}

impl FfmpegClient<'_> {
    fn build_thumbnail_output(input: &Path, output: &Path) -> Result<PathBuf> {
        let mut out_path = PathBuf::from(output);
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Failed to extract file stem for input: {}", input.display()))?;
        out_path.push(format!("{stem}-%04d"));
        out_path.set_extension("jpg");
        Ok(out_path)
    }

    fn build_metadata_command(&self, input: &Path) -> Command {
        let mut cmd = Command::new(&self.ffprobe_path);
        cmd.args(FFMPEG_QUIET);
        cmd.args(FFPROBE_FORMAT);
        cmd.arg(input);
        cmd
    }

    fn build_thumbnail_command(
        &self,
        input: &Path,
        output: &Path,
        scene_threshold: f32,
        width: Option<u16>,
    ) -> Result<Command> {
        let vf_str = match width {
            None => format!("select='gt(scene,{scene_threshold:.1})'"),
            Some(w) => format!("select='gt(scene,{scene_threshold:.1}),scale={w}:-2'"),
        };

        let output = Self::build_thumbnail_output(input, output)?;

        let mut cmd = Command::new(&self.ffmpeg_path);
        cmd.args(FFMPEG_QUIET);
        cmd.args(["-skip_frame", "nokey"]);
        cmd.args(FFMPEG_PROGRESS);
        cmd.arg("-i");
        cmd.arg(input);
        cmd.args(["-vf", &vf_str]);
        cmd.args(["-fps_mode", "vfr"]);
        cmd.args(["-q:v", "2"]);
        cmd.args(FFMPEG_OVERWRITE);
        cmd.arg(output);

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        Ok(cmd)
    }
}

pub struct ClientBuilder<'cb> {
    ffmpeg_path: &'cb Path,
    ffprobe_path: &'cb Path,
    executor: Option<Box<dyn CommandExecutor>>,
}

impl<'cb> ClientBuilder<'cb> {
    pub fn new() -> Self {
        Self {
            ffmpeg_path: &Path::new("ffmpeg"),
            ffprobe_path: &Path::new("ffprobe"),
            executor: None,
        }
    }

    pub fn ffmpeg_path<'p: 'cb>(mut self, path: &'p Path) -> Self {
        self.ffmpeg_path = path;
        self
    }

    pub fn ffprobe_path<'p: 'cb>(mut self, path: &'p Path) -> Self {
        self.ffprobe_path = path;
        self
    }

    pub fn executor(mut self, executor: Box<dyn CommandExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn build(self) -> FfmpegClient<'cb> {
        FfmpegClient {
            ffmpeg_path: self.ffmpeg_path,
            ffprobe_path: self.ffprobe_path,
            executor: self
                .executor
                .unwrap_or_else(|| Box::new(DefaultCommandExecutor)),
        }
    }
}
