use std::{
    cmp::{Ordering, max},
    fmt,
    str::FromStr,
};

/// 朝向枚举
#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum Orientation {
    Landscape, // 横屏
    Portrait,  // 竖屏
}

/// 分辨率枚举，包含常见标准分辨率及任意自定义分辨率。命名约定：`V` 前缀表示竖屏（Vertical）版本
#[derive(Debug, PartialEq, Clone, Copy, Default, Eq)]
pub enum Resolution {
    /// 3840x2160 (4K UHD 横屏)
    Uhd,
    /// 2160x3840 (4K UHD 竖屏)
    Vuhd,
    /// 2560x1440 (2K QHD 横屏)
    Qhd,
    /// 1440x2560 (2K QHD 竖屏)
    Vqhd,
    /// 1920x1080 (1080p 横屏)
    #[default]
    Fhd,
    /// 1080x1920 (1080p 竖屏)
    Vfhd,
    /// 1280x720 (720p 横屏)
    Hd,
    /// 720x1280 (720p 竖屏)
    Vhd,
    /// 自定义分辨率
    Arbitrary { width: u16, height: u16 },
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolutionError {
    #[error("Width or height can't be zero")]
    Zero,
    #[error("Delimiter x not found")]
    NoDelimiterX,
    #[error("Missing width")]
    MissingWidth,
    #[error("Missing height")]
    MissingHeight,
    #[error("Failed to parse {0}")]
    Parse(String),
}

impl FromStr for Resolution {
    type Err = ResolutionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (w_str, h_str) = s.split_once('x').ok_or(ResolutionError::NoDelimiterX)?;
        if w_str.is_empty() {
            return Err(ResolutionError::MissingWidth);
        }
        if h_str.is_empty() {
            return Err(ResolutionError::MissingHeight);
        }
        let width = w_str
            .parse::<u16>()
            .map_err(|_| ResolutionError::Parse(w_str.to_string()))?;
        let height = h_str
            .parse::<u16>()
            .map_err(|_| ResolutionError::Parse(h_str.to_string()))?;
        Resolution::new(width, height)
    }
}

impl Resolution {
    /// 根据宽高创建分辨率，自动匹配标准分辨率或返回 `Arbitrary`
    pub fn new(width: u16, height: u16) -> Result<Self, ResolutionError> {
        if width == 0 || height == 0 {
            return Err(ResolutionError::Zero);
        }
        Ok(match (width, height) {
            (3_840, 2_160) => Self::Uhd,
            (2_160, 3_840) => Self::Vuhd,
            (2_560, 1_440) => Self::Qhd,
            (1_440, 2_560) => Self::Vqhd,
            (1_920, 1_080) => Self::Fhd,
            (1_080, 1_920) => Self::Vfhd,
            (1_280, 720) => Self::Hd,
            (720, 1_280) => Self::Vhd,
            _ => Self::Arbitrary { width, height },
        })
    }

    /// 返回像素总数（宽×高）
    pub fn pixels(self) -> u32 {
        u32::from(self.width()) * u32::from(self.height())
    }

    /// 返回宽度
    pub fn width(self) -> u16 {
        match self {
            Self::Uhd => 3_840,
            Self::Vuhd => 2_160,
            Self::Qhd => 2_560,
            Self::Vqhd => 1_440,
            Self::Fhd => 1_920,
            Self::Vfhd => 1_080,
            Self::Hd => 1_280,
            Self::Vhd => 720,
            Self::Arbitrary { width, height: _ } => width,
        }
    }

    /// 返回高度
    pub fn height(self) -> u16 {
        match self {
            Self::Uhd => 2_160,
            Self::Vuhd => 3_840,
            Self::Qhd => 1_440,
            Self::Vqhd => 2_560,
            Self::Fhd => 1_080,
            Self::Vfhd => 1_920,
            Self::Hd => 720,
            Self::Vhd => 1_280,
            Self::Arbitrary { width: _, height } => height,
        }
    }

    /// 判断视频朝向（横屏或竖屏）,当宽高相等时视为横屏（Landscape）
    pub fn get_orientation(self) -> Orientation {
        match self.width().cmp(&self.height()) {
            Ordering::Greater | Ordering::Equal => Orientation::Landscape,
            Ordering::Less => Orientation::Portrait,
        }
    }

    /// 获取宽高中的较大值，用于缩放时的主维度
    pub fn get_primary_dimension(self) -> u16 {
        max(self.width(), self.height())
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Uhd => write!(f, "3840x2160"),
            Self::Vuhd => write!(f, "2160x3840"),
            Self::Qhd => write!(f, "2560x1440"),
            Self::Vqhd => write!(f, "1440x2560"),
            Self::Fhd => write!(f, "1920x1080"),
            Self::Vfhd => write!(f, "1080x1920"),
            Self::Hd => write!(f, "1280x720"),
            Self::Vhd => write!(f, "720x1280"),
            Self::Arbitrary { width, height } => write!(f, "{width}x{height}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_correct_variants() {
        assert_eq!(Resolution::new(3840, 2160).unwrap(), Resolution::Uhd);
        assert_eq!(Resolution::new(2160, 3840).unwrap(), Resolution::Vuhd);
        assert_eq!(Resolution::new(2560, 1440).unwrap(), Resolution::Qhd);
        assert_eq!(Resolution::new(1440, 2560).unwrap(), Resolution::Vqhd);
        assert_eq!(Resolution::new(1920, 1080).unwrap(), Resolution::Fhd);
        assert_eq!(Resolution::new(1080, 1920).unwrap(), Resolution::Vfhd);
        assert_eq!(Resolution::new(1280, 720).unwrap(), Resolution::Hd);
        assert_eq!(Resolution::new(720, 1280).unwrap(), Resolution::Vhd);
        let arb = Resolution::new(640, 480).unwrap();
        assert_eq!(
            arb,
            Resolution::Arbitrary {
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn new_rejects_zero_dimensions() {
        assert!(Resolution::new(0, 1920).is_err());
        assert!(Resolution::new(1920, 0).is_err());
    }

    #[test]
    fn pixels_computes_correctly() {
        assert_eq!(Resolution::Uhd.pixels(), 3840 * 2160);
        assert_eq!(Resolution::Vuhd.pixels(), 2160 * 3840);
        assert_eq!(Resolution::Qhd.pixels(), 2560 * 1440);
        assert_eq!(Resolution::Fhd.pixels(), 1920 * 1080);
        assert_eq!(Resolution::Hd.pixels(), 1280 * 720);
        let arb = Resolution::Arbitrary {
            width: 100,
            height: 200,
        };
        assert_eq!(arb.pixels(), 20000);
    }

    #[test]
    fn width_and_height_are_correct() {
        assert_eq!(Resolution::Uhd.width(), 3840);
        assert_eq!(Resolution::Uhd.height(), 2160);
        assert_eq!(Resolution::Vuhd.width(), 2160);
        assert_eq!(Resolution::Vuhd.height(), 3840);
        let arb = Resolution::Arbitrary {
            width: 640,
            height: 480,
        };
        assert_eq!(arb.width(), 640);
        assert_eq!(arb.height(), 480);
    }

    #[test]
    fn orientation_is_detected_correctly() {
        assert_eq!(Resolution::Uhd.get_orientation(), Orientation::Landscape);
        assert_eq!(Resolution::Vuhd.get_orientation(), Orientation::Portrait);
        let square = Resolution::Arbitrary {
            width: 100,
            height: 100,
        };
        assert_eq!(square.get_orientation(), Orientation::Landscape);
    }

    #[test]
    fn primary_dimension_returns_max() {
        assert_eq!(Resolution::Uhd.get_primary_dimension(), 3840);
        assert_eq!(Resolution::Vuhd.get_primary_dimension(), 3840);
        assert_eq!(
            Resolution::Arbitrary {
                width: 640,
                height: 480
            }
            .get_primary_dimension(),
            640
        );
    }

    #[test]
    fn from_str_parses_standard_and_arbitrary() {
        assert_eq!("3840x2160".parse::<Resolution>().unwrap(), Resolution::Uhd);
        assert_eq!("2160x3840".parse::<Resolution>().unwrap(), Resolution::Vuhd);
        assert_eq!("1920x1080".parse::<Resolution>().unwrap(), Resolution::Fhd);
        assert_eq!(
            "640x480".parse::<Resolution>().unwrap(),
            Resolution::Arbitrary {
                width: 640,
                height: 480
            }
        );
    }

    #[test]
    fn from_str_handles_errors() {
        assert!("".parse::<Resolution>().is_err());
        assert!("1920".parse::<Resolution>().is_err());
        assert!("1920x".parse::<Resolution>().is_err());
        assert!("x1080".parse::<Resolution>().is_err());
        assert!("1920xabc".parse::<Resolution>().is_err());
        assert!("0x1080".parse::<Resolution>().is_err());
        assert!("1920x0".parse::<Resolution>().is_err());
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(Resolution::Uhd.to_string(), "3840x2160");
        assert_eq!(Resolution::Vuhd.to_string(), "2160x3840");
        assert_eq!(
            Resolution::Arbitrary {
                width: 100,
                height: 200
            }
            .to_string(),
            "100x200"
        );
    }
}
