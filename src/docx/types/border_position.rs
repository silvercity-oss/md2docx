#[derive(Debug, Clone, PartialEq)]
pub enum TableBorderPosition {
    Left,
    Right,
    Top,
    Bottom,
    InsideH,
    InsideV,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableCellBorderPosition {
    Left,
    Right,
    Top,
    Bottom,
    InsideH,
    InsideV,
    Tl2br,
    Tr2bl,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParagraphBorderPosition {
    Left,
    Right,
    Top,
    Bottom,
    Between,
    Bar,
}
