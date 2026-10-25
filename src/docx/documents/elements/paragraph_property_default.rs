use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphPropertyDefault {
    paragraph_property: ParagraphProperty,
}

impl ParagraphPropertyDefault {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.paragraph_property = self.paragraph_property.line_spacing(spacing);
        self
    }
}

impl Default for ParagraphPropertyDefault {
    fn default() -> Self {
        let paragraph_property = ParagraphProperty::new();
        ParagraphPropertyDefault { paragraph_property }
    }
}

impl BuildXML for ParagraphPropertyDefault {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_paragraph_property_default()?
            .add_child(&self.paragraph_property)?
            .close()?
            .into_inner()
    }
}
