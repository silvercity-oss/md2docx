use std::fmt;

use super::errors;
use std::str::FromStr;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum HeightRule {
    Auto,
    #[default]
    AtLeast,
    Exact,
}

impl fmt::Display for HeightRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HeightRule::Auto => write!(f, "auto"),
            HeightRule::AtLeast => write!(f, "atLeast"),
            HeightRule::Exact => write!(f, "exact"),
        }
    }
}

impl FromStr for HeightRule {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(HeightRule::Auto),
            "atLeast" => Ok(HeightRule::AtLeast),
            "exact" => Ok(HeightRule::Exact),
            _ => Ok(HeightRule::AtLeast),
        }
    }
}
