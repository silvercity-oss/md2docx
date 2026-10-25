use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphStyle {
    pub val: String,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        ParagraphStyle {
            val: "Normal".to_owned(),
        }
    }
}

// 17.9.23
// pStyle (Paragraph Style's Associated Numbering Level)
// This element specifies the name of a paragraph style which shall automatically this numbering level when
// applied to the contents of the document. When a paragraph style is defined to include a numbering definition,
// any numbering level defined by the numPr element (§17.3.1.19) shall be ignored, and instead this element shall
// specify the numbering level associated with that paragraph style.
impl ParagraphStyle {
    pub fn new(val: Option<impl Into<String>>) -> ParagraphStyle {
        if let Some(v) = val {
            ParagraphStyle {
                val: escape_owned(v.into()),
            }
        } else {
            Default::default()
        }
    }
}

impl BuildXML for ParagraphStyle {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).paragraph_style(&self.val)?.into_inner()
    }
}
