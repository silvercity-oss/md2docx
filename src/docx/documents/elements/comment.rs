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
    pub fn new(id: usize) -> Comment {
        Self {
            id,
            ..Default::default()
        }
    }

    pub fn author(mut self, author: impl Into<String>) -> Comment {
        self.author = author.into();
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> Comment {
        self.date = date.into();
        self
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        self.children.push(CommentChild::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> Self {
        self.children.push(CommentChild::Table(Box::new(t)));
        self
    }

    pub fn parent_comment_id(mut self, parent_comment_id: usize) -> Comment {
        self.parent_comment_id = Some(parent_comment_id);
        self
    }

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

