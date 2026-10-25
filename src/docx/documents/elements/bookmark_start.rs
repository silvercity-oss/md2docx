use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkStart {
    pub id: usize,
    pub name: String,
}

impl BookmarkStart {
    pub fn new(id: usize, name: impl Into<String>) -> BookmarkStart {
        BookmarkStart { id, name: name.into() }
    }
}

impl BuildXML for BookmarkStart {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .bookmark_start(&self.id.to_string(), &self.name)?
            .into_inner()
    }
}
