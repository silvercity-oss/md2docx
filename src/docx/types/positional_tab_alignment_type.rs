
use std::fmt;
use std::str::FromStr;
use super::errors;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PositionalTabAlignmentType {
    Center,
    Left,
    Right,
}

impl fmt::Display for PositionalTabAlignmentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PositionalTabAlignmentType::Center => write!(f, "center"),
            PositionalTabAlignmentType::Left => write!(f, "left"),
            PositionalTabAlignmentType::Right => write!(f, "right"),
        }
    }
}

impl FromStr for PositionalTabAlignmentType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "center" => Ok(PositionalTabAlignmentType::Center),
            "right" => Ok(PositionalTabAlignmentType::Right),
            "left" => Ok(PositionalTabAlignmentType::Left),
            _ => Err(errors::TypeError::Unsupported(s.to_string())),
        }
    }
}
