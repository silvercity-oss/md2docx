use std::fmt;

use super::errors;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlignmentType {
    Auto,
    Baseline,
    Bottom,
    Center,
    Top,
}

impl fmt::Display for TextAlignmentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TextAlignmentType::Auto => write!(f, "auto"),
            TextAlignmentType::Baseline => write!(f, "baseline"),
            TextAlignmentType::Bottom => write!(f, "bottom"),
            TextAlignmentType::Center => write!(f, "center"),
            TextAlignmentType::Top => write!(f, "top"),
        }
    }
}

impl FromStr for TextAlignmentType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(TextAlignmentType::Auto),
            "baseline" => Ok(TextAlignmentType::Baseline),
            "bottom" => Ok(TextAlignmentType::Bottom),
            "center" => Ok(TextAlignmentType::Center),
            "top" => Ok(TextAlignmentType::Top),
            _ => Err(errors::TypeError::FromStrError),
        }
    }
}
