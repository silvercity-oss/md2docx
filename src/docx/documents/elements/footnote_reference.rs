use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::{xml_builder::*, Footnote, Paragraph};

#[derive(Debug, Clone, PartialEq)]

pub struct FootnoteReference {
    pub id: usize,
    pub custom_mark_follows: Option<bool>,
    pub style: String,
    pub content: Vec<Paragraph>,
}

impl FootnoteReference {}
impl From<Footnote> for FootnoteReference {
    fn from(footnote: Footnote) -> Self {
        FootnoteReference {
            id: footnote.id,
            custom_mark_follows: None,
            style: "FootnoteReference".to_string(),
            content: footnote.content,
        }
    }
}

impl BuildXML for FootnoteReference {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .footnote_reference(self.id, self.custom_mark_follows)?
            .into_inner()
    }
}
