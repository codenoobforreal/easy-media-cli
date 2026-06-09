mod progress_bar;

use crate::{event::Event, executor::Executor, ui::progress_bar::render_progress_bar};
use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand,
    cursor::Hide,
    terminal::{Clear, ClearType},
};
use std::{
    io::{Stdout, Write, stdout},
    time::Duration,
};
use tokio::{select, sync::broadcast, time::interval};

const OVERVIEW_TITLE: &str = "===== Tasks Overview =====";
const PROGRESS_TITLE: &str = "===== Task Progress =====";
const COMPLETE_TITLE: &str = "===== Process Complete =====";
const FAILED_LIST_TITLE: &str = "List of Failed Tasks:";
const SUCCESS_MSG: &str = "All tasks were processed successfully!";
const RENDER_INTERVAL: Duration = Duration::from_millis(100);

pub struct Ui {
    executor: Executor,
    event_receiver: broadcast::Receiver<Event>,
}

impl Ui {
    pub fn new(executor: Executor, event_receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            executor,
            event_receiver,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tick_interval = interval(RENDER_INTERVAL);
        let mut stdout = stdout();
        loop {
            select! {
                _ = tick_interval.tick() => {
                    self.render(&mut stdout).await?;
                }
                event = self.event_receiver.recv() => {
                    match event {
                        Ok(Event::AllTasksCompleted) | Ok(Event::Shutdown) => {
                            self.render_final(&mut stdout).await?;
                            break;
                        }
                        Err(_)=> {
                            break;
                        }
                        Ok(_) => {}
                    }
                }
            }
        }
        Ok(())
    }

    async fn render_running_tasks(&self, stdout: &mut Stdout) -> Result<()> {
        let running_tasks = self.executor.get_running_tasks().await;
        writeln!(stdout, "{PROGRESS_TITLE}")?;
        for task in running_tasks {
            writeln!(stdout, "{}", task.name())?;
            if let Some(file_name) = task.file_name() {
                writeln!(stdout, "{}", file_name)?;
            }
            let progress = task.progress();
            render_progress_bar(stdout, &progress)
                .with_context(|| format!("Failed to render progress bar"))?;
        }
        Ok(())
    }

    async fn render_overall_stats(&self, stdout: &mut Stdout) -> Result<()> {
        let (total, pending, running, completed, failed) = self.executor.get_task_stats().await;
        writeln!(stdout, "{OVERVIEW_TITLE}")?;
        writeln!(
            stdout,
            "Total tasks: {} | Completed: {} | Failed: {} | Running: {} | Pending: {}",
            total, completed, failed, running, pending,
        )?;
        Ok(())
    }

    async fn render_complete_stat(&self, stdout: &mut Stdout) -> Result<()> {
        let (total, _, _, completed, failed) = self.executor.get_task_stats().await;
        writeln!(stdout, "{COMPLETE_TITLE}")?;
        writeln!(
            stdout,
            "All Tasks: {} | Success: {} | Failed: {}",
            total, completed, failed
        )?;
        Ok(())
    }

    async fn render_failed_tasks(&self, stdout: &mut Stdout) -> Result<()> {
        let failed_tasks = self.executor.get_failed_tasks().await;
        if failed_tasks.is_empty() {
            return Ok(());
        }
        writeln!(stdout, "{FAILED_LIST_TITLE}")?;
        for task in failed_tasks {
            let file_name = task
                .file_name()
                .with_context(|| format!("Failed to get task file_name"))?;
            write!(
                stdout,
                "[{}] {}:\n{}\n",
                task.name(),
                file_name,
                task.error().unwrap_or_default()
            )?;
        }
        Ok(())
    }

    async fn render(&self, stdout: &mut Stdout) -> Result<()> {
        stdout.queue(Clear(ClearType::All))?.queue(Hide)?;
        self.render_overall_stats(stdout)
            .await
            .with_context(|| format!("Failed to render overall stats"))?;
        self.render_running_tasks(stdout)
            .await
            .with_context(|| format!("Failed to render running tasks"))?;
        // note: flush is required
        stdout.flush()?;
        Ok(())
    }

    async fn render_final(&self, stdout: &mut Stdout) -> Result<()> {
        stdout.queue(Clear(ClearType::All))?;
        self.render_complete_stat(stdout)
            .await
            .with_context(|| format!("Failed to render complete stats"))?;
        self.render_failed_tasks(stdout)
            .await
            .with_context(|| format!("Failed to render running tasks"))?;
        writeln!(stdout, "{SUCCESS_MSG}")?;
        stdout.flush()?;
        Ok(())
    }
}
