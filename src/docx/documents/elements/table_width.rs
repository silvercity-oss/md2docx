use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TableWidth {
    width: usize,
    width_type: WidthType,
}

impl TableWidth {
    pub fn new(width: usize, width_type: WidthType) -> TableWidth {
        TableWidth { width, width_type }
    }
}

impl BuildXML for TableWidth {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .table_width(self.width as i32, self.width_type)?
            .into_inner()
    }
}

