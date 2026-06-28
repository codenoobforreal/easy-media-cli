use crate::{
    domain::task::{Status, TaskMetadata},
    ui::{FAILED_LIST_TITLE, RESULT_LIST_TITLE, progress_bar::render_progress_bar, state::Stats},
};
use anyhow::{Context, Result};
use crossterm::{
    QueueableCommand,
    cursor::{Hide, MoveUp, Show},
    terminal::{Clear, ClearType},
};
use std::io::{Stderr, Stdout, Write, stderr, stdout};

/// 所有 UI 实现（终端、Web、GUI 等）都必须实现此特性。核心引擎仅通过此接口触发渲染，并与具体实现完全解耦
pub trait Renderer: Send + Sync {
    fn render_running(&mut self, stats: &Stats, tasks: &[Option<TaskMetadata>]) -> Result<()>;

    fn render_final(
        &mut self,
        stats: &Stats,
        tasks: &[Option<TaskMetadata>],
        message: &str,
    ) -> Result<()>;
}

/// 实现终端局部刷新，仅处理终端渲染细节
#[derive(Debug)]
pub struct DefaultRenderer<O: Write = Stdout, E: Write = Stderr> {
    stdout: O,
    stderr: E,
    /// 上一次渲染占用的逻辑行数，用于光标回退
    last_ui_lines: u16,
    buffer: Vec<u8>,
    stderr_buffer: Vec<u8>,
}

impl Default for DefaultRenderer<Stdout, Stderr> {
    fn default() -> Self {
        Self {
            stdout: stdout(),
            stderr: stderr(),
            last_ui_lines: 0,
            buffer: vec![],
            stderr_buffer: vec![],
        }
    }
}

impl<O: Write, E: Write> DefaultRenderer<O, E> {
    pub fn new(stdout: O, stderr: E) -> Self {
        Self {
            stdout,
            stderr,
            last_ui_lines: 0,
            buffer: vec![],
            stderr_buffer: vec![],
        }
    }
}

impl<O: Write, E: Write> DefaultRenderer<O, E> {
    fn write_overall_stats(w: &mut impl Write, stats: &Stats) -> Result<()> {
        if stats.expected_total() > 0 {
            write!(w, "Total: {}/{} | ", stats.total(), stats.expected_total())?;
        } else {
            write!(w, "Total: {} | ", stats.total())?;
        }

        writeln!(
            w,
            "Completed: {} | Failed: {} | Running: {} | Pending: {} | Canceled: {}",
            stats.completed(),
            stats.failed(),
            stats.running(),
            stats.pending(),
            stats.canceled()
        )?;

        Ok(())
    }

    fn write_running_tasks(w: &mut impl Write, tasks: &[Option<TaskMetadata>]) -> Result<()> {
        for metadata in tasks
            .iter()
            .flatten()
            .filter(|t| t.status() == Status::Running)
        {
            writeln!(w, "\n{}", metadata.name())?;
            render_progress_bar(w, metadata.progress().as_ref())
                .with_context(|| "Failed to render progress bar")?;
        }
        Ok(())
    }

    fn write_failed_tasks(w: &mut impl Write, tasks: &[Option<TaskMetadata>]) -> Result<()> {
        let failed: Vec<_> = tasks
            .iter()
            .flatten()
            .filter(|t| t.status() == Status::Failed)
            .collect();
        if failed.is_empty() {
            return Ok(());
        }
        writeln!(w, "{FAILED_LIST_TITLE}")?;
        for metadata in failed {
            write!(
                w,
                "[{}]:\n{}\n",
                metadata.name(),
                metadata.error().unwrap_or_default()
            )?;
        }
        Ok(())
    }

    fn write_complete_stat(w: &mut impl Write, stats: &Stats) -> Result<()> {
        if stats.expected_total() > 0 {
            write!(w, "Total: {}/{} | ", stats.total(), stats.expected_total())?;
        } else {
            write!(w, "Total: {} | ", stats.total())?;
        }

        writeln!(
            w,
            "Completed: {} | Failed: {} | Cancelled: {} | Not Started: {}",
            stats.completed(),
            stats.failed(),
            stats.canceled(),
            stats.pending()
        )?;

        Ok(())
    }

    fn write_task_results(w: &mut impl Write, tasks: &[Option<TaskMetadata>]) -> Result<()> {
        let with_result: Vec<_> = tasks
            .iter()
            .flatten()
            .filter(|t| t.result().is_some())
            .collect();
        if with_result.is_empty() {
            return Ok(());
        }
        writeln!(w, "\n{RESULT_LIST_TITLE}")?;
        for metadata in with_result {
            if let Some(result) = metadata.result() {
                write!(w, "\n[{}]:\n{result}\n", metadata.name())?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    /// 缓冲区中换行数量
    #[allow(clippy::naive_bytecount, clippy::cast_possible_truncation)]
    fn count_lines(buffer: &[u8]) -> u16 {
        // buffer.iter().filter(|&&b| b == b'\n').count() as u16
        let mut lines = buffer.iter().filter(|&&b| b == b'\n').count();
        if !buffer.is_empty() && buffer.last() != Some(&b'\n') {
            lines += 1; // 最后一行不完整也算一行
        }
        lines.clamp(0, u16::MAX as usize) as u16
    }
}

impl<O: Write + Send + Sync, E: Write + Send + Sync> Renderer for DefaultRenderer<O, E> {
    fn render_running(&mut self, stats: &Stats, tasks: &[Option<TaskMetadata>]) -> Result<()> {
        let result = (|| -> Result<()> {
            if self.last_ui_lines > 0 {
                self.stdout.queue(MoveUp(self.last_ui_lines))?;
            }
            self.stdout
                .queue(Clear(ClearType::FromCursorDown))?
                .queue(Hide)?;

            self.buffer.clear();
            Self::write_overall_stats(&mut self.buffer, stats)?;
            Self::write_running_tasks(&mut self.buffer, tasks)?;
            self.stdout.write_all(&self.buffer)?;
            self.stdout.flush()?;
            self.last_ui_lines = Self::count_lines(&self.buffer);
            Ok(())
        })();

        if result.is_err() {
            // 仅在出错时尝试恢复光标
            let _ = self.stdout.queue(Show);
            let _ = self.stdout.flush();
        }

        result
    }

    fn render_final(
        &mut self,
        stats: &Stats,
        tasks: &[Option<TaskMetadata>],
        message: &str,
    ) -> Result<()> {
        let result = (|| -> Result<()> {
            if self.last_ui_lines > 0 {
                self.stdout.queue(MoveUp(self.last_ui_lines))?;
            }
            self.stdout.queue(Clear(ClearType::FromCursorDown))?;

            self.buffer.clear();
            self.stderr_buffer.clear();

            Self::write_complete_stat(&mut self.buffer, stats)?;
            Self::write_task_results(&mut self.buffer, tasks)?;
            Self::write_failed_tasks(&mut self.stderr_buffer, tasks)?;
            writeln!(self.buffer, "\n{message}")?;

            self.stdout.write_all(&self.buffer)?;
            self.stderr.write_all(&self.stderr_buffer)?;
            self.stdout.flush()?;
            self.stderr.flush()?;
            Ok(())
        })();

        if result.is_err() {
            let _ = self.stdout.queue(Show);
            let _ = self.stdout.flush();
        }

        result
    }
}

/// 自动恢复光标，防止异常退出后终端光标隐藏
impl<O: Write, E: Write> Drop for DefaultRenderer<O, E> {
    fn drop(&mut self) {
        let _ = self.stdout.queue(Show);
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
    }
}
