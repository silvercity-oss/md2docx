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
pub struct TableBorder {
    pub border_type: BorderType,
    pub size: usize,
    pub color: String,
    position: TableBorderPosition,
    space: usize,
}

impl TableBorder {
    pub fn new(position: TableBorderPosition) -> TableBorder {
        TableBorder {
            position,
            border_type: BorderType::Single,
            size: 2,
            space: 0,
            color: "000000".to_owned(),
        }
    }
}

impl BuildXML for TableBorder {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let func = match self.position {
            TableBorderPosition::Top => XMLBuilder::border_top,
            TableBorderPosition::Left => XMLBuilder::border_left,
            TableBorderPosition::Bottom => XMLBuilder::border_bottom,
            TableBorderPosition::Right => XMLBuilder::border_right,
            TableBorderPosition::InsideH => XMLBuilder::border_inside_h,
            TableBorderPosition::InsideV => XMLBuilder::border_inside_v,
        };

        XMLBuilder::from(stream)
            .apply(|b| func(b, self.border_type, self.size, self.space, &self.color))?
            .into_inner()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableBorders {
    top: Option<TableBorder>,
    left: Option<TableBorder>,
    bottom: Option<TableBorder>,
    right: Option<TableBorder>,
    inside_h: Option<TableBorder>,
    inside_v: Option<TableBorder>,
}

impl Default for TableBorders {
    fn default() -> TableBorders {
        TableBorders {
            top: Some(TableBorder::new(TableBorderPosition::Top)),
            left: Some(TableBorder::new(TableBorderPosition::Left)),
            bottom: Some(TableBorder::new(TableBorderPosition::Bottom)),
            right: Some(TableBorder::new(TableBorderPosition::Right)),
            inside_h: Some(TableBorder::new(TableBorderPosition::InsideH)),
            inside_v: Some(TableBorder::new(TableBorderPosition::InsideV)),
        }
    }
}

impl TableBorders {
    pub fn new() -> TableBorders {
        Default::default()
    }
}

impl BuildXML for TableBorders {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_borders()?
            .add_optional_child(&self.top)?
            .add_optional_child(&self.left)?
            .add_optional_child(&self.bottom)?
            .add_optional_child(&self.right)?
            .add_optional_child(&self.inside_h)?
            .add_optional_child(&self.inside_v)?
            .close()?
            .into_inner()
    }
}
