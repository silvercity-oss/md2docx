use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TableIndent {
    width: i32,
    width_type: WidthType,
}

impl TableIndent {
    pub fn new(width: i32, width_type: WidthType) -> TableIndent {
        TableIndent { width, width_type }
    }
}

impl BuildXML for TableIndent {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .table_indent(self.width, self.width_type)?
            .into_inner()
    }
}

