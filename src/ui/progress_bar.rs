//! 进度条渲染工具

use crate::ffmpeg_progress::Progress;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg_progress::make_progress;
    use insta::assert_debug_snapshot;
    use std::{io, time::Duration};

    #[test]
    fn zero_percent_renders_empty_bar() {
        let mut buf = Vec::new();
        let prog = make_progress(0.0, Duration::from_secs(0), Some(Duration::from_secs(100)));
        render_progress_bar(&mut buf, Some(&prog)).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""[                              ] 0.0%\nTime used: 00:00:00 | ETA: 00:01:40\n""#);
        // assert!(out.contains("[                              ] 0.0%"));
        // assert!(out.contains("Time used: 00:00:00 | ETA: 00:01:40"));
    }

    #[test]
    fn fifty_percent_renders_half_filled() {
        let mut buf = Vec::new();
        let prog = make_progress(50.0, Duration::from_secs(30), Some(Duration::from_secs(30)));
        render_progress_bar(&mut buf, Some(&prog)).unwrap();
        let out = String::from_utf8(buf).unwrap();
        // 30 格总长度，50% 对应 15 个等号
        assert_debug_snapshot!(out,@r#""[===============               ] 50.0%\nTime used: 00:00:30 | ETA: 00:00:30\n""#);
        // assert!(out.contains("[===============               ] 50.0%"));
    }

    #[test]
    fn one_hundred_percent_renders_full_bar() {
        let mut buf = Vec::new();
        let prog = make_progress(100.0, Duration::from_mins(1), Some(Duration::ZERO));
        render_progress_bar(&mut buf, Some(&prog)).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""[==============================] 100.0%\nTime used: 00:01:00 | ETA: 00:00:00\n""#);
        // assert!(out.contains("[==============================] 100.0%"));
    }

    #[test]
    fn over_100_percent_is_clamped() {
        let mut buf = Vec::new();
        let prog = make_progress(150.0, Duration::from_mins(1), None);
        render_progress_bar(&mut buf, Some(&prog)).unwrap();
        let out = String::from_utf8(buf).unwrap();
        // 进度条被 clamp 到 30 格满，百分比仍显示原始值
        assert_debug_snapshot!(out,@r#""[==============================] 150.0%\nTime used: 00:01:00 | ETA: --:--:--\n""#);
        // assert!(out.contains("[==============================] 150.0%"));
    }

    #[test]
    fn no_progress_shows_placeholder() {
        let mut buf = Vec::new();
        render_progress_bar(&mut buf, None).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""[                              ] No progress data\nTime used: --:--:-- | ETA: --:--:--\n""#);
        // assert!(out.contains("[                              ] No progress data"));
        // assert!(out.contains("Time used: --:--:-- | ETA: --:--:--"));
    }

    #[test]
    fn eta_none_shows_placeholder() {
        let mut buf = Vec::new();
        let prog = make_progress(20.0, Duration::from_secs(10), None);
        render_progress_bar(&mut buf, Some(&prog)).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_debug_snapshot!(out,@r#""[======                        ] 20.0%\nTime used: 00:00:10 | ETA: --:--:--\n""#);
        // assert!(out.contains("ETA: --:--:--"));
    }

    #[test]
    fn writer_error_propagates_up() {
        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("write failed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush failed"))
            }
        }
        let mut w = BrokenWriter;
        assert!(render_progress_bar(&mut w, None).is_err());
    }
}
