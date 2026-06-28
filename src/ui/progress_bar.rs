//! 进度条渲染工具

use crate::domain::progress::Progress;
use anyhow::Result;
use std::{io::Write, time::Duration};

const BAR_LENGTH: usize = 30;

fn write_duration<W: Write>(writer: &mut W, d: Duration) -> Result<()> {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    write!(writer, "{hours:02}:{minutes:02}:{seconds:02}")?;
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub fn render_progress_bar<W: Write>(writer: &mut W, progress: Option<&Progress>) -> Result<()> {
    if let Some(p) = progress {
        let filled =
            (p.percentage() / 100.0 * BAR_LENGTH as f32).clamp(0.0, BAR_LENGTH as f32) as usize;

        write!(writer, "[")?;
        for _ in 0..filled {
            write!(writer, "=")?;
        }
        for _ in filled..BAR_LENGTH {
            write!(writer, " ")?;
        }
        writeln!(writer, "] {:.1}%", p.percentage())?;

        write!(writer, "Time used: ")?;
        write_duration(writer, p.elapsed())?;
        write!(writer, " | ETA: ")?;
        if let Some(eta) = p.eta() {
            write_duration(writer, eta)?;
            writeln!(writer)?;
        } else {
            writeln!(writer, "--:--:--")?;
        }
    } else {
        write!(writer, "[")?;
        for _ in 0..BAR_LENGTH {
            write!(writer, " ")?;
        }
        writeln!(writer, "] No progress data")?;
        writeln!(writer, "Time used: --:--:-- | ETA: --:--:--")?;
    }

    Ok(())
}
