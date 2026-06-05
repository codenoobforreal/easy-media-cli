use crate::{common::format_duration, task::Progress};
use std::io::{Stdout, Write};

const BAR_LENGTH: usize = 30;

pub fn render_progress_bar(stdout: &mut Stdout, progress: &Progress) -> () {
    let filled = (progress.percentage() / 100.0 * BAR_LENGTH as f32) as usize;
    let bar = format!(
        "[{}{}]",
        "=".repeat(filled),
        " ".repeat(BAR_LENGTH - filled)
    );
    let eta_str = progress
        .eta()
        .map_or("--:--:--".to_string(), format_duration);
    writeln!(stdout, "{bar} {:.1}% ", progress.percentage()).unwrap();
    writeln!(
        stdout,
        "Time used: {} | ETA: {}",
        format_duration(progress.elapsed()),
        eta_str
    )
    .unwrap();

    ()
}
