//! 跨领域通用工具

mod error;
mod format;
mod media_scan;
mod parser;

pub use error::join_errors_with_summary;
pub use format::{UnitSystem, format_duration_all, human_readable_size};
pub use media_scan::{collect_videos, is_video_file};
pub use parser::parse_float_str;

/// 相对误差浮点数相等判断，`rel_tol`：允许相对误差（如 0.001 = 0.1%）
#[cfg(test)]
pub fn approx_eq(a: f64, b: f64, rel_tol: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    let max_abs = a.abs().max(b.abs());
    // 同时兼容极小值，避免分母过小爆炸
    diff < rel_tol * max_abs || diff < f64::MIN_POSITIVE
}
