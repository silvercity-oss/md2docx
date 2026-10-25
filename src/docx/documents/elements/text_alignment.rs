use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::{xml_builder::*, TextAlignmentType};

#[derive(Debug, Clone, PartialEq)]
pub struct TextAlignment(pub TextAlignmentType);

impl TextAlignment {
    pub fn new(val: TextAlignmentType) -> TextAlignment {
        TextAlignment(val)
    }
}

impl BuildXML for TextAlignment {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .text_alignment(&format!("{}", self.0))?
            .into_inner()
    }
}

