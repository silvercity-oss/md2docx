use std::io::Write;

use super::*;

use crate::docx::documents::{BuildXML, HistoryId, Run};
use crate::docx::{escape, xml_builder::*};

#[derive(Debug, Clone, PartialEq)]
pub enum InsertChild {
    Run(Box<Run>),
    Delete(Delete),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
}

impl BuildXML for InsertChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            InsertChild::Run(v) => v.build_to(stream),
            InsertChild::Delete(v) => v.build_to(stream),
            InsertChild::CommentStart(v) => v.build_to(stream),
            InsertChild::CommentEnd(v) => v.build_to(stream),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    pub children: Vec<InsertChild>,
    pub author: String,
    pub date: String,
}

impl Default for Insert {
    fn default() -> Insert {
        Insert {
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            children: vec![],
        }
    }
}

impl Insert {
    pub fn new(run: Run) -> Insert {
        Self {
            children: vec![InsertChild::Run(Box::new(run))],
            ..Default::default()
        }
    }

    pub fn new_with_empty() -> Insert {
        Self {
            ..Default::default()
        }
    }

    pub fn new_with_del(del: Delete) -> Insert {
        Self {
            children: vec![InsertChild::Delete(del)],
            ..Default::default()
        }
    }

    pub fn add_run(mut self, run: Run) -> Insert {
        self.children.push(InsertChild::Run(Box::new(run)));
        self
    }

    pub fn add_delete(mut self, del: Delete) -> Insert {
        self.children.push(InsertChild::Delete(del));
        self
    }

    pub fn add_child(mut self, c: InsertChild) -> Insert {
        self.children.push(c);
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> Self {
        self.children
            .push(InsertChild::CommentStart(Box::new(CommentRangeStart::new(
                comment,
            ))));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> Self {
        self.children
            .push(InsertChild::CommentEnd(CommentRangeEnd::new(id)));
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> Insert {
        self.author = escape::escape_owned(author.into());
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> Insert {
        self.date = date.into();
        self
    }
}

impl HistoryId for Insert {}

impl BuildXML for Insert {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = self.generate();
        XMLBuilder::from(stream)
            .open_insert(id.as_ref(), &self.author, &self.date)?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}

