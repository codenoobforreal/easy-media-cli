mod progress_bar;

use crate::{
    error::AppResult, event::Event, executor::Executor, ui::progress_bar::render_progress_bar,
};
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

    pub async fn run(&mut self) -> AppResult<()> {
        let mut tick_interval = interval(RENDER_INTERVAL);
        let mut stdout = stdout();
        loop {
            select! {
                _ = tick_interval.tick() => {
                    self.render(&mut stdout).await?;
                }
                event = self.event_receiver.recv() => {
                    match event {
                        Ok(Event::AllTasksCompleted) => {
                            self.render_final(&mut stdout).await?;
                            break;
                        }
                        Ok(Event::Shutdown) | Err(_)=> {
                            break;
                        }
                        Ok(_) => {}

                    }
                }
            }
        }
        Ok(())
    }

    async fn render_running_tasks(&self, stdout: &mut Stdout) -> () {
        let running_tasks = self.executor.get_running_tasks().await;
        writeln!(stdout, "{PROGRESS_TITLE}").unwrap();
        for task in running_tasks {
            writeln!(stdout, "{}", task.name()).unwrap();
            if let Some(file_name) = task.file_name() {
                writeln!(stdout, "{}", file_name).unwrap();
            }
            let progress = task.progress();
            render_progress_bar(stdout, &progress);
        }
        ()
    }

    async fn render_overall_stats(&self, stdout: &mut Stdout) -> () {
        let (total, pending, running, completed, failed) = self.executor.get_task_stats().await;
        writeln!(stdout, "{OVERVIEW_TITLE}").unwrap();
        writeln!(
            stdout,
            "Total tasks: {} | Completed: {} | Failed: {} | Running: {} | Pending: {}",
            total, completed, failed, running, pending,
        )
        .unwrap();
        ()
    }

    async fn render_complete_stat(&self, stdout: &mut Stdout) -> () {
        let (total, _, _, completed, failed) = self.executor.get_task_stats().await;
        writeln!(stdout, "{COMPLETE_TITLE}").unwrap();
        writeln!(
            stdout,
            "All Tasks: {} | Success: {} | Failed: {}",
            total, completed, failed
        )
        .unwrap();
        ()
    }

    async fn render_failed_tasks(&self, stdout: &mut Stdout) -> () {
        let failed_tasks = self.executor.get_failed_tasks().await;

        if failed_tasks.is_empty() {
            return ();
        }

        writeln!(stdout, "{FAILED_LIST_TITLE}").unwrap();
        for task in failed_tasks {
            let file_name = task.file_name().unwrap_or_default();
            write!(
                stdout,
                "[{}] {}:\n{}\n",
                task.name(),
                file_name,
                task.error().unwrap_or_default()
            )
            .unwrap();
        }
        ()
    }

    async fn render(&self, stdout: &mut Stdout) -> AppResult<()> {
        stdout.queue(Clear(ClearType::All))?.queue(Hide).unwrap();
        self.render_overall_stats(stdout).await;
        self.render_running_tasks(stdout).await;
        // note: flush is required
        stdout.flush().unwrap();
        Ok(())
    }

    async fn render_final(&self, stdout: &mut Stdout) -> AppResult<()> {
        stdout.queue(Clear(ClearType::All)).unwrap();
        self.render_complete_stat(stdout).await;
        self.render_failed_tasks(stdout).await;
        writeln!(stdout, "{SUCCESS_MSG}").unwrap();
        stdout.flush().unwrap();
        Ok(())
    }
}
