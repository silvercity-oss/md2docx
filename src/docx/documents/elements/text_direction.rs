use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TextDirection {
    val: TextDirectionType,
}

impl TextDirection {
    pub fn new(t: TextDirectionType) -> TextDirection {
        TextDirection { val: t }
    }
}

impl BuildXML for TextDirection {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .text_direction(&self.val.to_string())?
            .into_inner()
    }
}

