//! 跨领域通用工具

use anyhow::anyhow;
use std::{fmt, time::Duration};

pub mod media_scan;

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

/// 同时支持分数和十进制格式
pub fn parse_float_str(float_str: impl AsRef<str>) -> Option<f64> {
    let s = float_str.as_ref().trim();

    if let Some((num_str, den_str)) = s.split_once('/') {
        let numerator: f64 = num_str.trim().parse().ok()?;
        let denominator: f64 = den_str.trim().parse().ok()?;
        (denominator.abs() >= f64::EPSILON).then_some(numerator / denominator)
    } else {
        s.parse().ok()
    }
}

/// 字节大小单位制式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSystem {
    /// 二进制单位，基于 1024 进制（IEC 标准）
    Binary,
    /// 十进制单位，基于 1000 进制（SI 国际单位制）
    Decimal,
}

/// 将字节数转换为人类易读的字符串，自动选择合适的单位，并修剪尾部无意义的零。
///
/// # 参数
/// - `bytes`：原始字节数（`u64` 类型）
/// - `system`：单位制式，二进制（MiB, GiB...）或十进制（MB, GB...）
///
/// # 返回值
/// 返回格式化的字符串，如 `"9.75 MiB"`、`"1 KB"`、`"2.3 MB"`
#[allow(clippy::cast_precision_loss)]
pub fn human_readable_size_with(bytes: u64, system: UnitSystem) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }

    let (base, units) = match system {
        UnitSystem::Binary => (1024.0, ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"]),
        UnitSystem::Decimal => (1000.0, ["B", "kB", "MB", "GB", "TB", "PB", "EB"]),
    };

    let mut size = bytes as f64;
    let mut unit_index = 0;

    // 当数值大于等于基数且还有更大的单位时，向上进位
    while size >= base && unit_index < units.len() - 1 {
        size /= base;
        unit_index += 1;
    }

    // 格式化数值部分：智能修剪多余的尾随零
    // - 如果是整数（如 1.00 -> "1"），显示为 "1"
    // - 如果是一位小数（如 1.20 -> "1.2"），显示为 "1.2"
    // - 否则显示两位小数（如 9.75 -> "9.75"）
    let formatted_value = if size.fract() == 0.0 {
        format!("{size:.0}")
    } else {
        // 先保留两位小数，然后去除末尾的 '0'，再去除末尾可能留下的 '.'
        let s = format!("{size:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };

    format!("{} {}", formatted_value, units[unit_index])
}

/// 以二进制单位（1024 进制）将字节数转换为人类易读字符串。
///
/// 这是 `human_readable_size_with(bytes, UnitSystem::Binary)` 的便捷封装。
pub fn human_readable_size(bytes: u64) -> String {
    human_readable_size_with(bytes, UnitSystem::Binary)
}

/// 将 `Duration` 格式化为所有非零整数秒单位的字符串。
///
/// 忽略秒以下的部分（毫秒、微秒、纳秒），不足 1 秒时直接返回 `"0s"`。
///
pub fn format_duration_all(d: Duration) -> String {
    let total_secs = d.as_secs();

    if total_secs == 0 {
        return "0s".to_string();
    }

    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();

    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}min"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }

    parts.join(" ")
}

/// 将多个错误合并为一个 `anyhow::Error`，并在首行展示自定义的摘要信息。
///
/// `summary` 会被放在最终错误消息的第一行。
/// 后续每一行会以 `-` 开头，展示对应错误的 `{:#?}` 美化格式。
pub fn join_errors_with_summary<E: fmt::Debug>(
    summary: impl Into<String>,
    errors: &[E],
) -> anyhow::Error {
    let summary = summary.into();
    let mut lines = Vec::with_capacity(1 + errors.len());
    lines.push(summary);
    for e in errors {
        lines.push(format!("- {e:#?}"));
    }
    anyhow!("{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_parse_float_str() {
        assert_eq!(parse_float_str("3.3"), Some(3.3));
        assert_eq!(parse_float_str("  3.3  "), Some(3.3));
        assert_eq!(parse_float_str("1/2"), Some(0.5));
        assert_eq!(parse_float_str(" 3 / 4 "), Some(0.75));
        assert_eq!(parse_float_str("0/1"), Some(0.0));
        assert_eq!(parse_float_str("1/0"), None);
        assert_eq!(parse_float_str("abc"), None);
        assert_eq!(parse_float_str("1/"), None);
        assert_eq!(parse_float_str("/2"), None);
    }

    #[test]
    fn test_human_readable_size_with_binary() {
        assert_eq!(human_readable_size_with(0, UnitSystem::Binary), "0 B");
        assert_eq!(human_readable_size_with(1024, UnitSystem::Binary), "1 KiB");
        assert_eq!(
            human_readable_size_with(1024 * 1024, UnitSystem::Binary),
            "1 MiB"
        );
        assert_eq!(
            human_readable_size_with(1024 * 1024 * 1024, UnitSystem::Binary),
            "1 GiB"
        );
        assert_eq!(
            human_readable_size_with(1500, UnitSystem::Binary),
            "1.46 KiB"
        );
        assert_eq!(
            human_readable_size_with(1536, UnitSystem::Binary),
            "1.5 KiB"
        );
        assert_eq!(human_readable_size_with(2048, UnitSystem::Binary), "2 KiB");
    }

    #[test]
    fn test_human_readable_size_with_decimal() {
        assert_eq!(human_readable_size_with(0, UnitSystem::Decimal), "0 B");
        assert_eq!(human_readable_size_with(1000, UnitSystem::Decimal), "1 kB");
        assert_eq!(
            human_readable_size_with(1000 * 1000, UnitSystem::Decimal),
            "1 MB"
        );
        assert_eq!(
            human_readable_size_with(1500, UnitSystem::Decimal),
            "1.5 kB"
        );
        assert_eq!(human_readable_size_with(2000, UnitSystem::Decimal), "2 kB");
    }

    #[test]
    fn test_human_readable_size_default() {
        assert_eq!(human_readable_size(1024), "1 KiB");
    }

    #[test]
    fn test_format_duration_all() {
        assert_eq!(format_duration_all(Duration::ZERO), "0s");
        assert_eq!(format_duration_all(Duration::from_secs(1)), "1s");
        assert_eq!(format_duration_all(Duration::from_mins(1)), "1min");
        assert_eq!(format_duration_all(Duration::from_secs(61)), "1min 1s");
        assert_eq!(format_duration_all(Duration::from_hours(1)), "1h");
        assert_eq!(format_duration_all(Duration::from_secs(3661)), "1h 1min 1s");
        assert_eq!(format_duration_all(Duration::from_hours(24)), "1d");
        assert_eq!(
            format_duration_all(Duration::from_secs(90061)),
            "1d 1h 1min 1s"
        );
    }

    #[test]
    fn test_join_errors_with_summary() {
        let errors = vec!["err1".to_string(), "err2".to_string()];
        let err = join_errors_with_summary("summary", &errors);
        let msg = err.to_string();
        assert_snapshot!(msg,@r#"
        summary
        - "err1"
        - "err2"
        "#);
    }
}
