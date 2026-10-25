use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;
use footnote_id::generate_footnote_id;

#[derive(Debug, Clone, PartialEq)]
pub struct Footnote {
    pub id: usize,
    pub content: Vec<FootnoteChild>,
}

/// A block inside a footnote: a paragraph or a table.
#[derive(Debug, Clone, PartialEq)]
pub enum FootnoteChild {
    Paragraph(Paragraph),
    Table(Box<Table>),
}

impl BuildXML for FootnoteChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            FootnoteChild::Paragraph(p) => p.build_to(stream),
            FootnoteChild::Table(t) => t.build_to(stream),
        }
    }
}

impl Default for Footnote {
    fn default() -> Self {
        Footnote { id: 1, content: vec![] }
    }
}

impl Footnote {
    pub fn new() -> Self {
        Self {
            id: generate_footnote_id(),
            ..Default::default()
        }
    }

    pub fn add_content(&mut self, p: Paragraph) {
        self.content.push(FootnoteChild::Paragraph(p));
    }

    pub fn add_table(&mut self, t: Table) {
        self.content.push(FootnoteChild::Table(Box::new(t)));
    }
}
impl From<&FootnoteReference> for Footnote {
    fn from(reference: &FootnoteReference) -> Self {
        Footnote {
            id: reference.id,
            content: reference.content.clone(),
        }
    }
}

impl BuildXML for Footnote {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        // To ensure docx compatible XML serialization for footnotes, we default to an empty paragraph.
        let empty;
        let content = if self.content.is_empty() {
            empty = vec![FootnoteChild::Paragraph(Paragraph::new())];
            &empty
        } else {
            &self.content
        };

        XMLBuilder::from(stream)
            .open_footnote(&format!("{}", self.id))?
            .add_children(content)?
            .close()?
            .into_inner()
    }
}
