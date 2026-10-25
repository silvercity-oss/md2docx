use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;
use footnote_id::generate_footnote_id;

#[derive(Debug, Clone, PartialEq)]
pub struct Footnote {
    pub id: usize,
    pub content: Vec<Paragraph>,
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

    pub fn add_content(&mut self, p: Paragraph) -> Self {
        self.content.push(p);
        self.clone()
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
        let mut footnote = self.clone();
        if self.content == vec![] {
            eprintln!("{footnote:?}");
            footnote.add_content(Paragraph::new());
        }

        XMLBuilder::from(stream)
            .open_footnote(&format!("{}", self.id))?
            .add_children(&footnote.content)?
            .close()?
            .into_inner()
    }
}
