use crate::{
    domain::{Status, TaskMetadata},
    ui::{FAILED_LIST_TITLE, RESULT_LIST_TITLE, Stats, render_progress_bar},
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
    fn render_running(&mut self, stats: &Stats, tasks: &[(usize, &TaskMetadata)]) -> Result<()>;

    fn render_final(
        &mut self,
        stats: &Stats,
        tasks: &[(usize, &TaskMetadata)],
        message: &str,
    ) -> Result<()>;
}

/// 终端渲染器：实现终端局部刷新，支持自定义输出流用于单元测试；仅处理终端渲染细节，不维护业务状态
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

    fn write_running_tasks(w: &mut impl Write, tasks: &[(usize, &TaskMetadata)]) -> Result<()> {
        for (_, task) in tasks.iter().filter(|(_, t)| t.status() == Status::Running) {
            writeln!(w, "\n{}", task.name())?;
            render_progress_bar(w, task.progress().as_ref())
                .with_context(|| "Failed to render progress bar")?;
        }
        Ok(())
    }

    fn write_failed_tasks(w: &mut impl Write, tasks: &[(usize, &TaskMetadata)]) -> Result<()> {
        let failed: Vec<_> = tasks
            .iter()
            .filter(|(_, t)| t.status() == Status::Failed)
            .collect();
        if failed.is_empty() {
            return Ok(());
        }
        writeln!(w, "{FAILED_LIST_TITLE}")?;
        for (_, task) in failed {
            write!(
                w,
                "[{}]:\n{}\n",
                task.name(),
                task.error().unwrap_or_default()
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

    fn write_task_results(w: &mut impl Write, tasks: &[(usize, &TaskMetadata)]) -> Result<()> {
        let with_result: Vec<_> = tasks.iter().filter(|(_, t)| t.result().is_some()).collect();
        if with_result.is_empty() {
            return Ok(());
        }
        writeln!(w)?;
        writeln!(w, "{RESULT_LIST_TITLE}")?;
        for (_, task) in with_result {
            if let Some(result) = task.result() {
                writeln!(w, "[{}]:", task.name())?;
                writeln!(w, "{result}")?;
            }
        }
        writeln!(w)?;
        Ok(())
    }

    /// 返回缓冲区中的换行数量
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
    fn render_running(&mut self, stats: &Stats, tasks: &[(usize, &TaskMetadata)]) -> Result<()> {
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
        tasks: &[(usize, &TaskMetadata)],
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
            writeln!(self.buffer, "{message}")?;

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

/// 退出自动恢复光标，防止异常退出后终端光标隐藏
impl<O: Write, E: Write> Drop for DefaultRenderer<O, E> {
    fn drop(&mut self) {
        let _ = self.stdout.queue(Show);
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default)]
    pub struct MockRenderer {
        pub running_calls: Arc<Mutex<usize>>,
        pub final_calls: Arc<Mutex<usize>>,
        pub last_msg: Arc<Mutex<Option<String>>>,
    }

    impl Renderer for MockRenderer {
        fn render_running(
            &mut self,
            _stats: &Stats,
            _tasks: &[(usize, &TaskMetadata)],
        ) -> Result<()> {
            *self.running_calls.lock().unwrap() += 1;
            Ok(())
        }

        fn render_final(
            &mut self,
            _stats: &Stats,
            _tasks: &[(usize, &TaskMetadata)],
            message: &str,
        ) -> Result<()> {
            *self.final_calls.lock().unwrap() += 1;
            *self.last_msg.lock().unwrap() = Some(message.to_string());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::test_utils::sample_test_metadata_with_id_name, ui::test_utils::sample_stats,
    };
    use insta::assert_debug_snapshot;

    type MemRender = DefaultRenderer<Vec<u8>, Vec<u8>>;

    fn mem_renderer() -> MemRender {
        DefaultRenderer::<Vec<u8>, Vec<u8>>::new(vec![], vec![])
    }

    #[test]
    fn count_lines_counts_newlines_accurately() {
        assert_eq!(MemRender::count_lines(b""), 0);
        assert_eq!(MemRender::count_lines(b"no newline"), 1);
        assert_eq!(MemRender::count_lines(b"one\n"), 1);
        assert_eq!(MemRender::count_lines(b"a\nb\nc"), 3);
        assert_eq!(MemRender::count_lines(b"\n\n\n"), 3);
    }

    #[test]
    fn count_lines_clamps_to_u16_max() {
        let buf = vec![b'\n'; u16::MAX as usize + 100];
        assert_eq!(MemRender::count_lines(&buf), u16::MAX);
    }

    #[test]
    fn write_overall_stats_format_correct() {
        let mut buf = Vec::new();
        MemRender::write_overall_stats(&mut buf, &sample_stats()).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""Total: 0 | Completed: 0 | Failed: 0 | Running: 0 | Pending: 0 | Canceled: 0\n""#);
    }

    #[test]
    fn write_failed_tasks_empty_when_no_failures() {
        let mut buf = Vec::new();
        MemRender::write_failed_tasks(&mut buf, &[]).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn write_failed_tasks_only_lists_failed() {
        let mut failed_meta = sample_test_metadata_with_id_name(1, "bad_task");
        failed_meta.mark_failed("parse error".to_owned());
        let mut good_meta = sample_test_metadata_with_id_name(2, "good_task");
        good_meta.mark_completed(None);
        let tasks = vec![(1, &failed_meta), (2, &good_meta)];
        let mut buf = Vec::new();
        MemRender::write_failed_tasks(&mut buf, &tasks).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""List of failed tasks:\n[bad_task]:\nparse error\n""#);
    }

    #[test]
    fn write_task_results_empty_when_no_results() {
        let mut buf = Vec::new();
        MemRender::write_task_results(&mut buf, &[]).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn render_running_first_render_no_cursor_move() {
        let mut r = mem_renderer();
        r.render_running(&sample_stats(), &[]).unwrap();
        let stdout = String::from_utf8_lossy(&r.stdout);
        assert!(stdout.contains("Total: 0"));
        assert_eq!(r.last_ui_lines, 1);
    }

    #[test]
    fn render_running_second_render_moves_cursor_up() {
        let mut r = mem_renderer();
        r.render_running(&sample_stats(), &[]).unwrap();
        let first_lines = r.last_ui_lines;
        r.render_running(&sample_stats(), &[]).unwrap();
        // 第二次渲染会先回退光标，行数保持一致
        assert_eq!(r.last_ui_lines, first_lines);
    }

    #[test]
    fn drop_restores_cursor_visibility() {
        let r = mem_renderer();
        // Drop 时自动发送 Show 光标命令，无 panic 即语义正确
        drop(r);
    }
}
