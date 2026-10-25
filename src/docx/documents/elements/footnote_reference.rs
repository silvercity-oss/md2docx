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

impl FootnoteReference {
    pub fn new(id: usize) -> Self {
        FootnoteReference {
            id,
            custom_mark_follows: None,
            style: "FootnoteReference".to_string(),
            content: vec![],
        }
    }
    /// Suppress the automatic reference marker when a custom mark follows.
    pub fn custom_mark_follows(mut self, value: bool) -> Self {
        self.custom_mark_follows = Some(value);
        self
    }
    /// Add footnote content as a Paragraph
    pub fn footnote(&mut self, p: Paragraph) {
        self.content.push(p)
    }
}
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

