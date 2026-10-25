use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct VertAlign {
    val: VertAlignType,
}

impl VertAlign {
    pub fn new(val: VertAlignType) -> VertAlign {
        Self { val }
    }
}

impl BuildXML for VertAlign {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .vert_align(&self.val.to_string())?
            .into_inner()
    }
}
