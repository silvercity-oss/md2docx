
use std::fmt;
use std::str::FromStr;
use super::errors;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DocGridType {
    Default,
    Lines,
    LinesAndChars,
    SnapToChars,
}

impl fmt::Display for DocGridType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DocGridType::Default => write!(f, "default"),
            DocGridType::Lines => write!(f, "lines"),
            DocGridType::LinesAndChars => write!(f, "linesAndChars"),
            DocGridType::SnapToChars => write!(f, "snapToChars"),
        }
    }
}

impl FromStr for DocGridType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(DocGridType::Default),
            "lines" => Ok(DocGridType::Lines),
            "linesAndChars" => Ok(DocGridType::LinesAndChars),
            "snapToChars" => Ok(DocGridType::SnapToChars),
            _ => Err(errors::TypeError::Unknown),
        }
    }
}
