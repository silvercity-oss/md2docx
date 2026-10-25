use super::errors;
use std::fmt;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PageOrientationType {
    Landscape,
    Portrait,
}

impl fmt::Display for PageOrientationType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PageOrientationType::Landscape => write!(f, "landscape"),
            PageOrientationType::Portrait => write!(f, "portrait"),
        }
    }
}

impl FromStr for PageOrientationType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "landscape" => Ok(PageOrientationType::Landscape),
            "portrait" => Ok(PageOrientationType::Portrait),
            _ => Ok(PageOrientationType::Portrait),
        }
    }
}
