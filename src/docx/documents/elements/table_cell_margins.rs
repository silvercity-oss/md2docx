use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use crate::docx::CellMargin;

#[derive(Debug, Clone, PartialEq)]
pub struct TableCellMargins {
    top: CellMargin,
    left: CellMargin,
    bottom: CellMargin,
    right: CellMargin,
}

impl Default for TableCellMargins {
    fn default() -> TableCellMargins {
        TableCellMargins {
            top: CellMargin {
                val: 0,
                width_type: WidthType::Dxa,
            },
            left: CellMargin::default(),
            bottom: CellMargin {
                val: 0,
                width_type: WidthType::Dxa,
            },
            right: CellMargin::default(),
        }
    }
}

impl TableCellMargins {}

impl BuildXML for TableCellMargins {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_cell_margins()?
            .margin_top(self.top.val as i32, self.top.width_type)?
            .margin_left(self.left.val as i32, self.left.width_type)?
            .margin_bottom(self.bottom.val as i32, self.bottom.width_type)?
            .margin_right(self.right.val as i32, self.right.width_type)?
            .close()?
            .into_inner()
    }
}
