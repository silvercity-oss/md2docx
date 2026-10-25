use super::errors;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PositionalTabRelativeTo {
    Indent,
    Margin,
}

impl fmt::Display for PositionalTabRelativeTo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PositionalTabRelativeTo::Indent => write!(f, "indent"),
            PositionalTabRelativeTo::Margin => write!(f, "margin"),
        }
    }
}

impl FromStr for PositionalTabRelativeTo {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "indent" => Ok(PositionalTabRelativeTo::Indent),
            "margin" => Ok(PositionalTabRelativeTo::Margin),
            _ => Err(errors::TypeError::Unsupported(s.to_string())),
        }
    }
}
