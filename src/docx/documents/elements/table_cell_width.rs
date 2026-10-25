use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TableCellWidth {
    width: usize,
    width_type: WidthType,
}

impl TableCellWidth {}

impl BuildXML for TableCellWidth {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .table_cell_width(self.width as i32, self.width_type)?
            .into_inner()
    }
}
