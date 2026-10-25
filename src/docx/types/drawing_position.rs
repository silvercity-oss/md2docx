use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawingPositionType {
    Anchor,
    Inline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PicAlign {
    Left,
    Right,
    Center,
    Bottom,
    Top,
}

impl fmt::Display for PicAlign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PicAlign::Left => write!(f, "left"),
            PicAlign::Right => write!(f, "right"),
            PicAlign::Center => write!(f, "center"),
            PicAlign::Bottom => write!(f, "bottom"),
            PicAlign::Top => write!(f, "top"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawingPosition {
    Offset(i32),
    Align(PicAlign),
}
