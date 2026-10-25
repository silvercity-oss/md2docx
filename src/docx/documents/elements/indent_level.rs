use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct IndentLevel {
    pub val: usize,
}

impl IndentLevel {
    pub fn new(val: usize) -> IndentLevel {
        IndentLevel { val }
    }
}

impl BuildXML for IndentLevel {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .indent_level(self.val)?
            .into_inner()
    }
}

