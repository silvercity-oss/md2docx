use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TextBorder {
    pub border_type: BorderType,
    pub size: usize,
    pub color: String,
    pub space: usize,
}

impl TextBorder {}

impl Default for TextBorder {
    fn default() -> Self {
        TextBorder {
            border_type: BorderType::Single,
            size: 4,
            space: 0,
            color: "auto".to_owned(),
        }
    }
}

impl BuildXML for TextBorder {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .text_border(self.border_type, self.size, self.space, &self.color)?
            .into_inner()
    }
}
