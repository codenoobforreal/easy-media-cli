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
    /// 1920x1080 (1080p 横屏)，默认参数，如要更改请变更 `EvArgs`
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
    fn test_resolution_new() {
        assert_eq!(Resolution::new(3840, 2160).unwrap(), Resolution::Uhd);
        assert_eq!(Resolution::new(2160, 3840).unwrap(), Resolution::Vuhd);
        assert_eq!(Resolution::new(1920, 1080).unwrap(), Resolution::Fhd);
        assert_eq!(Resolution::new(1280, 720).unwrap(), Resolution::Hd);
        assert_eq!(
            Resolution::new(800, 600).unwrap(),
            Resolution::Arbitrary {
                width: 800,
                height: 600
            }
        );
        assert!(Resolution::new(0, 1920).is_err());
        assert!(Resolution::new(1920, 0).is_err());
    }

    #[test]
    fn test_resolution_from_str() {
        assert_eq!(Resolution::from_str("3840x2160").unwrap(), Resolution::Uhd);
        assert_eq!(Resolution::from_str("1920x1080").unwrap(), Resolution::Fhd);
        assert_eq!(
            Resolution::from_str("800x600").unwrap(),
            Resolution::Arbitrary {
                width: 800,
                height: 600
            }
        );
        assert!(Resolution::from_str("1920x").is_err());
        assert!(Resolution::from_str("x1080").is_err());
        assert!(Resolution::from_str("abcxdef").is_err());
        assert!(Resolution::from_str("0x1080").is_err());
    }

    #[test]
    fn test_resolution_pixels() {
        assert_eq!(Resolution::Uhd.pixels(), 3840 * 2160);
        assert_eq!(Resolution::Fhd.pixels(), 1920 * 1080);
        assert_eq!(
            Resolution::Arbitrary {
                width: 800,
                height: 600
            }
            .pixels(),
            480_000
        );
    }

    #[test]
    fn test_resolution_orientation() {
        assert_eq!(Resolution::Uhd.get_orientation(), Orientation::Landscape);
        assert_eq!(Resolution::Vuhd.get_orientation(), Orientation::Portrait);
        assert_eq!(Resolution::Fhd.get_orientation(), Orientation::Landscape);
        assert_eq!(
            Resolution::Arbitrary {
                width: 100,
                height: 200
            }
            .get_orientation(),
            Orientation::Portrait
        );
        assert_eq!(
            Resolution::Arbitrary {
                width: 200,
                height: 100
            }
            .get_orientation(),
            Orientation::Landscape
        );
        assert_eq!(
            Resolution::Arbitrary {
                width: 100,
                height: 100
            }
            .get_orientation(),
            Orientation::Landscape
        );
    }

    #[test]
    fn test_resolution_primary_dimension() {
        assert_eq!(Resolution::Uhd.get_primary_dimension(), 3840);
        assert_eq!(Resolution::Vuhd.get_primary_dimension(), 3840);
        assert_eq!(Resolution::Fhd.get_primary_dimension(), 1920);
        assert_eq!(
            Resolution::Arbitrary {
                width: 800,
                height: 600
            }
            .get_primary_dimension(),
            800
        );
    }

    #[test]
    fn test_resolution_display() {
        assert_eq!(Resolution::Uhd.to_string(), "3840x2160");
        assert_eq!(Resolution::Vuhd.to_string(), "2160x3840");
        assert_eq!(
            Resolution::Arbitrary {
                width: 800,
                height: 600
            }
            .to_string(),
            "800x600"
        );
    }
}
