use crate::{
    common::format_duration,
    progress::Progress,
    task::{Metadata, MetadataMap, NotifyEvent},
};
use anyhow::{Result as AnyResult, bail};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{Clear, ClearType},
};
use std::{
    ffi::OsString,
    fmt,
    io::{self, Stdout, Write},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

const PROGRESS_BAR_LENGTH: usize = 30;

const UI_OVERVIEW_TITLE: &str = "===== Tasks Overview =====";
const UI_PROGRESS_TITLE: &str = "===== Task Progress =====";
const UI_FAILED_TITLE: &str = "===== Failed Task =====";
const UI_COMPLETE_TITLE: &str = "===== Process Complete =====";
const UI_FAILED_LIST_TITLE: &str = "List of Failed Tasks:";
const UI_SUCCESS_MSG: &str = "All tasks were processed successfully!";

const RENDER_INTERVAL: Duration = Duration::from_millis(100);

pub struct Renderer {
    metadata: MetadataMap,
    stats: Stats,
    current_running: Option<(usize, Progress)>,
    failed_tasks: Vec<(OsString, OsString)>,
    receiver: Receiver<NotifyEvent>,
}

impl Renderer {
    pub fn new(metadata: MetadataMap, receiver: Receiver<NotifyEvent>) -> Self {
        let total = metadata.len();
        Self {
            metadata,
            stats: Stats::new(total),
            current_running: None,
            failed_tasks: Vec::new(),
            receiver,
        }
    }

    pub fn run(&mut self) -> AnyResult<()> {
        let mut stdout = io::stdout();
        execute!(stdout, Hide)?;

        loop {
            match self.receiver.recv_timeout(RENDER_INTERVAL) {
                Ok(msg) => self.handle_message(msg)?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("The task notification channel appears to be disconnected")
                }
            }

            self.render_frame(&mut stdout)?;
            if self.stats.is_all_finished() {
                break;
            }
        }

        execute!(stdout, Show)?;
        self.render_final_result(&mut stdout)?;

        Ok(())
    }

    fn render_final_result(&self, stdout: &mut io::Stdout) -> AnyResult<()> {
        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
        writeln!(stdout, "{UI_COMPLETE_TITLE}")?;
        writeln!(
            stdout,
            "All Tasks: {} | Success: {} | Failed: {}",
            self.stats.total(),
            self.stats.completed(),
            self.stats.failed()
        )?;

        if !self.failed_tasks.is_empty() {
            writeln!(stdout, "{UI_FAILED_LIST_TITLE}")?;
            self.failed_tasks.iter().for_each(|(name, error)| {
                let _ = writeln!(stdout, "- [{}]: {}", name.display(), error.display());
            });
        } else {
            writeln!(stdout, "{UI_SUCCESS_MSG}")?;
        }

        Ok(())
    }

    fn handle_message(&mut self, msg: NotifyEvent) -> AnyResult<()> {
        match msg {
            NotifyEvent::Running(id) => {
                self.stats.on_task_running();
                self.current_running = Some((id, Progress::default()));
            }

            NotifyEvent::Progress(id, progress) => {
                self.current_running = Some((id, progress));
            }

            NotifyEvent::Completed() => {
                self.stats.on_task_completed();
                self.current_running = None;
            }

            NotifyEvent::Failed(id, error) => {
                self.stats.on_task_failed();
                self.current_running = None;
                if let Some(meta) = self.metadata.get(&id) {
                    self.failed_tasks
                        .push((meta.name().to_os_string(), OsString::from(error)));
                }
            }

            NotifyEvent::Error(e) => bail!(e),
        }

        Ok(())
    }

    fn render_frame(&self, stdout: &mut Stdout) -> AnyResult<()> {
        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

        writeln!(stdout, "{UI_OVERVIEW_TITLE}")?;
        writeln!(
            stdout,
            "Total tasks: {} | Completed: {} | Failed: {} | Running: {} | Pending: {}",
            self.stats.total(),
            self.stats.completed(),
            self.stats.failed(),
            self.stats.running(),
            self.stats.pending()
        )?;

        writeln!(stdout, "{UI_PROGRESS_TITLE}")?;
        if let Some((id, progress)) = &self.current_running {
            if let Some(meta) = self.metadata.get(&id) {
                self.render_progress_bar(stdout, meta, progress)?;
            }
        }

        if !self.failed_tasks.is_empty() {
            writeln!(stdout, "{UI_FAILED_TITLE}")?;
            for (name, error) in &self.failed_tasks {
                writeln!(stdout, "{}: {}\n", name.display(), error.display())?;
            }
        }

        stdout.flush()?;
        Ok(())
    }

    fn render_progress_bar(
        &self,
        stdout: &mut Stdout,
        meta: &Metadata,
        progress: &Progress,
    ) -> AnyResult<()> {
        if !meta.supports_progress() {
            writeln!(stdout, "{}", meta.name().display())?;
            writeln!(stdout, "{} - Processing...", meta.task_type())?;
            return Ok(());
        }

        let filled = (progress.percentage() / 100.0 * PROGRESS_BAR_LENGTH as f32) as usize;
        let bar = format!(
            "[{}{}]",
            "=".repeat(filled),
            " ".repeat(PROGRESS_BAR_LENGTH - filled)
        );

        let eta_str = progress
            .eta()
            .map_or("--:--:--".to_string(), format_duration);
        writeln!(stdout, "{}", meta.name().display())?;
        writeln!(
            stdout,
            "{} - {bar} {:.1}%",
            meta.task_type(),
            progress.percentage()
        )?;
        writeln!(
            stdout,
            "Time used: {} | Estimated time remaining: {}",
            format_duration(progress.elapsed()),
            eta_str
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    total: usize,
    completed: usize,
    failed: usize,
    pending: usize,
    running: usize,
}

impl Stats {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            pending: total,
            ..Default::default()
        }
    }

    pub fn on_task_running(&mut self) {
        debug_assert!(self.pending > 0, "No pending tasks to run");
        debug_assert!(self.running == 0, "A task is already running");

        self.pending -= 1;
        self.running = 1;
    }

    pub fn on_task_completed(&mut self) {
        debug_assert!(self.running == 1, "No task is running");

        self.running = 0;
        self.completed += 1;
    }

    pub fn on_task_failed(&mut self) {
        debug_assert!(self.running == 1, "No task is running");

        self.running = 0;
        self.failed += 1;
    }

    pub fn is_all_finished(&self) -> bool {
        self.completed + self.failed == self.total
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn failed(&self) -> usize {
        self.failed
    }

    pub fn completed(&self) -> usize {
        self.completed
    }

    pub fn running(&self) -> usize {
        self.running
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total: {}, Completed: {}, Failed: {}, Pending: {}, Running: {}",
            self.total, self.completed, self.failed, self.pending, self.running
        )
    }
}
