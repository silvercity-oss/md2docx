use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CommentRangeEnd {
    id: usize,
}

impl CommentRangeEnd {
    pub fn new(id: usize) -> CommentRangeEnd {
        CommentRangeEnd { id }
    }
}

impl BuildXML for CommentRangeEnd {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_run()?
            .open_run_property()?
            .close()?
            .close()?
            .comment_range_end(&format!("{}", self.id))?
            .open_run()?
            .comment_reference(&format!("{}", self.id))?
            .close()?
            .into_inner()
    }
}

