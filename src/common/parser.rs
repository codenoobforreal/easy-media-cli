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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::approx_eq;
    use std::f64;

    #[test]
    fn parse_decimal_integer() {
        assert!(approx_eq(parse_float_str("25").unwrap(), 25.0, 1e-9));
        assert!(approx_eq(parse_float_str("0").unwrap(), 0.0, 1e-9));
    }

    #[test]
    fn parse_decimal_float() {
        assert!(approx_eq(
            parse_float_str("3.141592653589793").unwrap(),
            f64::consts::PI,
            1e-9
        ));
        assert!(approx_eq(parse_float_str("0.5").unwrap(), 0.5, 1e-9));
    }

    #[test]
    fn parse_fraction_format() {
        assert!(approx_eq(parse_float_str("25/1").unwrap(), 25.0, 1e-9));
        assert!(approx_eq(parse_float_str("1/2").unwrap(), 0.5, 1e-9));
        assert!(approx_eq(
            parse_float_str("30000/1001").unwrap(),
            30000.0 / 1001.0,
            1e-6
        ));
    }

    #[test]
    fn parse_zero_denominator_returns_none() {
        assert!(parse_float_str("1/0").is_none());
        assert!(parse_float_str("0/0").is_none());
    }

    #[test]
    fn parse_negative_number_works() {
        assert!(approx_eq(parse_float_str("-3.5").unwrap(), -3.5, 1e-9));
        assert!(approx_eq(parse_float_str("-1/2").unwrap(), -0.5, 1e-9));
    }

    #[test]
    fn parse_trims_whitespace() {
        assert!(approx_eq(parse_float_str("  25.5  ").unwrap(), 25.5, 1e-9));
        assert!(approx_eq(
            parse_float_str("  30 / 1  ").unwrap(),
            30.0,
            1e-9
        ));
    }

    #[test]
    fn parse_invalid_input_returns_none() {
        assert!(parse_float_str("").is_none());
        assert!(parse_float_str("abc").is_none());
        assert!(parse_float_str("12a3").is_none());
        assert!(parse_float_str("/").is_none());
        assert!(parse_float_str("1/").is_none());
        assert!(parse_float_str("/2").is_none());
    }

    #[test]
    fn parse_audio_zero_frame_rate_returns_none() {
        // 匹配 ffprobe 音频流 0/0 帧率的典型场景
        assert!(parse_float_str("0/0").is_none());
    }
}
