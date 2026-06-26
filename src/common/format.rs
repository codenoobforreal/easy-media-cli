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

use std::time::Duration;

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
