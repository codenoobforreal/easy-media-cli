use crate::infra::RawFfmpegProgress;
use anyhow::{Context, Result};

#[derive(Default)]
pub struct FfmpegProgressParser {
    out_time_ms: Option<u64>,
    speed: Option<f32>,
    total_size: Option<u64>,
}

impl FfmpegProgressParser {
    pub fn feed_line(&mut self, line: &str) -> Result<Option<RawFfmpegProgress>> {
        let trim_line = line.trim();
        if trim_line.is_empty() {
            return Ok(None);
        }

        let (key, value) = match trim_line.split_once('=') {
            Some((k, v)) if !v.is_empty() && v != "N/A" => (k, v),
            _ => return Ok(None),
        };

        match key {
            "total_size" => {
                self.total_size = Some(
                    value
                        .parse()
                        .with_context(|| "Failed to parse total_size")?,
                );
                Ok(None)
            }

            "out_time_ms" => {
                self.out_time_ms = Some(
                    value
                        .parse()
                        .with_context(|| format!("Failed to parse out_time_ms: {value}"))?,
                );
                Ok(None)
            }

            "speed" => {
                self.speed = Some(
                    value
                        .trim()
                        .strip_suffix('x')
                        .with_context(|| format!("Failed to parse speed: {value}"))?
                        .parse()
                        .with_context(|| format!("Failed to parse speed value: {value}"))?,
                );
                Ok(None)
            }

            "progress" => {
                let result = match (self.speed, self.out_time_ms) {
                    (Some(speed), Some(out_time_ms)) => {
                        let raw = RawFfmpegProgress::new(speed, out_time_ms);
                        self.out_time_ms = None;
                        self.speed = None;
                        Some(raw)
                    }
                    _ => None,
                };
                Ok(result)
            }

            _ => Ok(None),
        }
    }

    pub fn total_size(&self) -> Option<u64> {
        self.total_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::approx_eq;
    use insta::assert_snapshot;

    /// 正常顺序 `out_time_ms` → speed → progress 可正确解析
    #[test]
    fn test_parser_normal_sequence_returns_progress() {
        let mut parser = FfmpegProgressParser::default();
        let res1 = parser.feed_line("out_time_ms=1234567").unwrap();
        let res2 = parser.feed_line("speed=2.5x").unwrap();
        let res3 = parser.feed_line("progress=continue").unwrap();
        assert!(res1.is_none());
        assert!(res2.is_none());
        let raw = res3.expect("Should produce raw progress");
        assert!(approx_eq(f64::from(raw.speed()), 2.5, 0.001));
        assert_eq!(raw.out_time_ms(), 1_234_567);
    }

    /// 反向顺序 speed → `out_time_ms` → `progress` 同样可正常解析
    #[test]
    fn test_parser_reverse_field_sequence_returns_progress() {
        let mut parser = FfmpegProgressParser::default();
        parser.feed_line("speed=1.8x").unwrap();
        parser.feed_line("out_time_ms=5000000").unwrap();
        let raw = parser.feed_line("progress=continue").unwrap().unwrap();
        assert!(approx_eq(f64::from(raw.speed()), 1.8, 0.001));
        assert_eq!(raw.out_time_ms(), 5_000_000);
    }

    /// 仅单个字段缓存时，progress 行返回 None
    #[test]
    fn test_parser_progress_with_partial_cache_returns_none() {
        let mut parser = FfmpegProgressParser::default();
        // 仅缓存 out_time_ms
        parser.feed_line("out_time_ms=1000000").unwrap();
        assert!(parser.feed_line("progress=continue").unwrap().is_none());

        // 仅缓存 speed
        let mut parser2 = FfmpegProgressParser::default();
        parser2.feed_line("speed=1.0x").unwrap();
        assert!(parser2.feed_line("progress=continue").unwrap().is_none());
    }

    /// 空行、无等号行、N/A 值、空值行、带空格值均被正确跳过，且不影响缓存状态
    #[test]
    fn test_parser_ignores_invalid_lines() {
        let mut parser = FfmpegProgressParser::default();
        parser.feed_line("out_time_ms=1000000").unwrap();
        assert!(parser.feed_line("").unwrap().is_none());
        assert!(parser.feed_line("some_random_text").unwrap().is_none());
        assert!(parser.feed_line("speed=N/A").unwrap().is_none());
        assert!(parser.feed_line("out_time_ms=").unwrap().is_none());
        // speed 值带前置空格，验证 trim 逻辑
        assert!(parser.feed_line("speed= 1.0x").unwrap().is_none());
        // 无效行不破坏缓存，补充 speed 后仍可正常生成进度
        parser.feed_line("speed=1.0x").unwrap();
        let res = parser.feed_line("progress=continue").unwrap();
        assert!(res.is_some());
    }

    /// `out_time_ms` 非数字时返回解析错误，且不污染已缓存的其他字段
    #[test]
    fn test_parser_invalid_out_time_ms_returns_error() {
        let mut parser = FfmpegProgressParser::default();
        // 先缓存 speed
        parser.feed_line("speed=2.0x").unwrap();
        // 解析 out_time_ms 失败
        let res = parser.feed_line("out_time_ms=abc123");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert_snapshot!(err_msg,@"Failed to parse out_time_ms: abc123");

        // 缓存的 speed 未被破坏，补充正确的 out_time_ms 后仍可正常生成进度
        parser.feed_line("out_time_ms=1000000").unwrap();
        assert!(parser.feed_line("progress=continue").unwrap().is_some());
    }

    /// speed 缺少 x 后缀时返回解析错误
    #[test]
    fn test_parser_invalid_speed_format_returns_error() {
        let mut parser = FfmpegProgressParser::default();
        let res = parser.feed_line("speed=3.0");
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert_snapshot!(err_msg,@"Failed to parse speed: 3.0");
    }

    /// 生成进度后缓存清空，连续 progress 行返回 None；重新填充字段后可再次生成
    #[test]
    fn test_parser_clears_cache_after_progress() {
        let mut parser = FfmpegProgressParser::default();
        // 第一轮生成
        parser.feed_line("out_time_ms=1000").unwrap();
        parser.feed_line("speed=1.0x").unwrap();
        assert!(parser.feed_line("progress=continue").unwrap().is_some());
        // 空缓存下 progress 返回 None
        assert!(parser.feed_line("progress=continue").unwrap().is_none());

        // 第二轮重新填充，可正常生成
        parser.feed_line("out_time_ms=2000").unwrap();
        parser.feed_line("speed=1.5x").unwrap();
        let raw = parser.feed_line("progress=end").unwrap().unwrap();
        assert_eq!(raw.out_time_ms(), 2000);
        assert!(approx_eq(f64::from(raw.speed()), 1.5, 0.001));
    }

    /// 同名字段多次输入，后一次覆盖前一次缓存
    #[test]
    fn test_parser_later_field_overrides_previous() {
        let mut parser = FfmpegProgressParser::default();
        parser.feed_line("out_time_ms=1000").unwrap();
        parser.feed_line("out_time_ms=5000").unwrap(); // 覆盖前值
        parser.feed_line("speed=2.0x").unwrap();
        let raw = parser.feed_line("progress=continue").unwrap().unwrap();
        assert_eq!(raw.out_time_ms(), 5000);
    }

    /// 未知键名的行被静默忽略，不影响解析
    #[test]
    fn test_parser_ignores_unknown_keys() {
        let mut parser = FfmpegProgressParser::default();
        parser.feed_line("frame=1234").unwrap();
        parser.feed_line("fps=30.5").unwrap();
        parser.feed_line("bitrate=2000kbits/s").unwrap();

        parser.feed_line("out_time_ms=2000000").unwrap();
        parser.feed_line("speed=0.8x").unwrap();
        assert!(parser.feed_line("progress=end").unwrap().is_some());
    }
}
