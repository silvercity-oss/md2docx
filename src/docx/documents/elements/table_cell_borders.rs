use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

/*
    Please see. L.4.3.2.18 Cell Border Properties

    left – left border
    right – right border
    top – top border
    bottom – bottom border
    insideH – inner horizontal borders
    insideV – inner vertical borders
    tl2br – diagonal border from top left corner to bottom right corner
    tr2bl – diagonal border from top right corner to bottom left corner
*/
#[derive(Debug, Clone, PartialEq)]
pub struct TableCellBorder {
    pub border_type: BorderType,
    pub size: usize,
    pub color: String,
    position: TableCellBorderPosition,
    space: usize,
}

impl TableCellBorder {
    pub fn new(position: TableCellBorderPosition) -> TableCellBorder {
        TableCellBorder {
            position,
            border_type: BorderType::Single,
            size: 2,
            space: 0,
            color: "000000".to_owned(),
        }
    }
}

impl BuildXML for TableCellBorder {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let func = match self.position {
            TableCellBorderPosition::Top => XMLBuilder::border_top,
            TableCellBorderPosition::Left => XMLBuilder::border_left,
            TableCellBorderPosition::Bottom => XMLBuilder::border_bottom,
            TableCellBorderPosition::Right => XMLBuilder::border_right,
            TableCellBorderPosition::InsideH => XMLBuilder::border_inside_h,
            TableCellBorderPosition::InsideV => XMLBuilder::border_inside_v,
            TableCellBorderPosition::Tr2bl => XMLBuilder::border_tr2bl,
            TableCellBorderPosition::Tl2br => XMLBuilder::border_tl2br,
        };
        XMLBuilder::from(stream)
            .apply(|b| func(b, self.border_type, self.size, self.space, &self.color))?
            .into_inner()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCellBorders {
    top: Option<TableCellBorder>,
    left: Option<TableCellBorder>,
    bottom: Option<TableCellBorder>,
    right: Option<TableCellBorder>,
    inside_h: Option<TableCellBorder>,
    inside_v: Option<TableCellBorder>,
    tr2bl: Option<TableCellBorder>,
    tl2br: Option<TableCellBorder>,
}

impl Default for TableCellBorders {
    fn default() -> TableCellBorders {
        TableCellBorders {
            top: Some(TableCellBorder::new(TableCellBorderPosition::Top)),
            left: Some(TableCellBorder::new(TableCellBorderPosition::Left)),
            bottom: Some(TableCellBorder::new(TableCellBorderPosition::Bottom)),
            right: Some(TableCellBorder::new(TableCellBorderPosition::Right)),
            inside_h: Some(TableCellBorder::new(TableCellBorderPosition::InsideH)),
            inside_v: Some(TableCellBorder::new(TableCellBorderPosition::InsideV)),
            tr2bl: None,
            tl2br: None,
        }
    }
}

impl TableCellBorders {}

impl BuildXML for TableCellBorders {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_cell_borders()?
            .add_optional_child(&self.top)?
            .add_optional_child(&self.left)?
            .add_optional_child(&self.bottom)?
            .add_optional_child(&self.right)?
            .add_optional_child(&self.inside_h)?
            .add_optional_child(&self.inside_v)?
            .add_optional_child(&self.tl2br)?
            .add_optional_child(&self.tr2bl)?
            .close()?
            .into_inner()
    }
}
