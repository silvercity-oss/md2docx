use crate::docx::types::errors;
use crate::docx::TypeError;
use std::str::FromStr;
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LineSpacingType {
    Auto,
    AtLeast,
    Exact,
}

impl FromStr for LineSpacingType {
    type Err = errors::TypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(LineSpacingType::Auto),
            "atLeast" => Ok(LineSpacingType::AtLeast),
            "exact" => Ok(LineSpacingType::Exact),
            _ => Err(TypeError::FromStrError),
        }
    }
}
