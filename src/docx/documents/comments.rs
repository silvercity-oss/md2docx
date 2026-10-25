use super::Comment;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comments {
    pub(crate) comments: Vec<Comment>,
}

impl Comments {
    pub fn new() -> Self {
        Default::default()
    }

    pub(crate) fn add_comments(&mut self, comments: Vec<Comment>) {
        self.comments = comments;
    }
}

impl BuildXML for Comments {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_comments()?
            .add_children(&self.comments)?
            .close()?
            .into_inner()
    }
}
