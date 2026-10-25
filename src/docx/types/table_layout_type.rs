use super::errors;
use std::fmt;
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TableLayoutType {
    Fixed,
    Autofit,
}

impl fmt::Display for TableLayoutType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TableLayoutType::Fixed => write!(f, "fixed"),
            TableLayoutType::Autofit => write!(f, "autofit"),
        }
    }
}

impl FromStr for TableLayoutType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fixed" => Ok(TableLayoutType::Fixed),
            _ => Ok(TableLayoutType::Autofit),
        }
    }
}
