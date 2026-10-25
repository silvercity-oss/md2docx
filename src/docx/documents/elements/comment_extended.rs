use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CommentExtended {
    pub paragraph_id: String,
    pub done: bool,
    pub parent_paragraph_id: Option<String>,
}

impl CommentExtended {
    pub fn new(paragraph_id: impl Into<String>) -> CommentExtended {
        Self {
            paragraph_id: paragraph_id.into(),
            done: false,
            parent_paragraph_id: None,
        }
    }

    pub fn parent_paragraph_id(mut self, id: impl Into<String>) -> CommentExtended {
        self.parent_paragraph_id = Some(id.into());
        self
    }
}

impl BuildXML for CommentExtended {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .comment_extended(&self.paragraph_id, self.done, &self.parent_paragraph_id)?
            .into_inner()
    }
}
