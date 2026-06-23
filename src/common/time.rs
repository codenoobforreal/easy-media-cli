use std::time::Duration;

/// 将一个 Duration 转换为 `HH:MM:SS` 格式的字符串
#[allow(dead_code)]
pub fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn zero_duration_formats_correctly() {
        assert_snapshot!(format_duration(Duration::ZERO), @"00:00:00");
    }

    #[test]
    fn seconds_only_pads_leading_zero() {
        assert_snapshot!(format_duration(Duration::from_secs(5)), @"00:00:05");
        assert_snapshot!(format_duration(Duration::from_secs(59)), @"00:00:59");
    }

    #[test]
    fn minutes_and_seconds_format() {
        assert_snapshot!(format_duration(Duration::from_secs(125)), @"00:02:05"); // 2分5秒
        assert_snapshot!(format_duration(Duration::from_secs(3599)), @"00:59:59");
    }

    #[test]
    fn hours_minutes_seconds_format() {
        assert_snapshot!(format_duration(Duration::from_secs(3661)), @"01:01:01"); // 1小时1分1秒
        assert_snapshot!(format_duration(Duration::from_hours(2)), @"02:00:00");
    }

    #[test]
    fn over_24_hours_works_normally() {
        assert_snapshot!(format_duration(Duration::from_secs(90061)), @"25:01:01"); // 25小时1分1秒
    }

    #[test]
    fn subsecond_duration_truncates_to_whole_seconds() {
        assert_snapshot!(format_duration(Duration::from_millis(1500)), @"00:00:01");
        assert_snapshot!(format_duration(Duration::from_millis(999)), @"00:00:00");
    }
}
