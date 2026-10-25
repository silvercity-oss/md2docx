use super::Footnote;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq, Default)]
pub struct Footnotes {
    pub(crate) footnotes: Vec<Footnote>,
}

impl Footnotes {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add(&mut self, footnotes: Vec<Footnote>) {
        self.footnotes.extend(footnotes)
    }
}

impl BuildXML for Footnotes {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_footnotes()?
            .add_children(&self.footnotes)?
            .close()?
            .into_inner()
    }
}

