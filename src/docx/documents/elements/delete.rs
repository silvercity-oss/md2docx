use std::io::Write;

use crate::docx::xml_builder::*;
use crate::docx::{documents::*, escape};

#[derive(Debug, Clone, PartialEq)]
pub struct Delete {
    pub author: String,
    pub date: String,
    pub children: Vec<DeleteChild>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeleteChild {
    Run(Box<Run>),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
}

impl Default for Delete {
    fn default() -> Delete {
        Delete {
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            children: vec![],
        }
    }
}

impl Delete {
    pub fn new() -> Delete {
        Self {
            children: vec![],
            ..Default::default()
        }
    }

    pub fn add_run(mut self, run: Run) -> Delete {
        self.children.push(DeleteChild::Run(Box::new(run)));
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> Delete {
        self.children
            .push(DeleteChild::CommentStart(Box::new(CommentRangeStart::new(
                comment,
            ))));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> Delete {
        self.children
            .push(DeleteChild::CommentEnd(CommentRangeEnd::new(id)));
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> Delete {
        self.author = escape::escape_owned(author.into());
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> Delete {
        self.date = date.into();
        self
    }
}

impl HistoryId for Delete {}

impl BuildXML for Delete {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = self.generate();
        XMLBuilder::from(stream)
            .open_delete(id.as_ref(), &self.author, &self.date)?
            .apply_each(&self.children, |ch, b| match ch {
                DeleteChild::Run(t) => b.add_child(&**t),
                DeleteChild::CommentStart(c) => b.add_child(&c),
                DeleteChild::CommentEnd(c) => b.add_child(c),
            })?
            .close()?
            .into_inner()
    }
}

