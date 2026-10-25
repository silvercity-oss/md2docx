use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkEnd {
    pub id: usize,
}

impl BookmarkEnd {
    pub fn new(id: usize) -> BookmarkEnd {
        BookmarkEnd { id }
    }
}

impl BuildXML for BookmarkEnd {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .bookmark_end(&self.id.to_string())?
            .into_inner()
    }
}
