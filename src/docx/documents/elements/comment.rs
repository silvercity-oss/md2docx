use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub id: usize,
    pub author: String,
    pub date: String,
    pub children: Vec<CommentChild>,
    pub parent_comment_id: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommentChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

impl Default for Comment {
    fn default() -> Comment {
        Comment {
            id: 1,
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            children: vec![],
            parent_comment_id: None,
        }
    }
}

impl Comment {
    pub fn id(&self) -> usize {
        self.id
    }
}

impl BuildXML for CommentChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            CommentChild::Paragraph(v) => v.as_ref().build_to(stream),
            CommentChild::Table(v) => v.as_ref().build_to(stream),
        }
    }
}

impl BuildXML for Comment {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_comment(&self.id.to_string(), &self.author, &self.date, "")?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}
