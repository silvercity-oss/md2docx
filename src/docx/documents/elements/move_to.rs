use std::io::Write;

use super::*;

use crate::docx::documents::{BuildXML, HistoryId, Run};
use crate::docx::{escape, xml_builder::*};

#[derive(Debug, Clone, PartialEq)]
pub enum MoveToChild {
    Run(Box<Run>),
    Delete(Delete),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
}

impl BuildXML for MoveToChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            MoveToChild::Run(v) => v.build_to(stream),
            MoveToChild::Delete(v) => v.build_to(stream),
            MoveToChild::CommentStart(v) => v.build_to(stream),
            MoveToChild::CommentEnd(v) => v.build_to(stream),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveTo {
    pub children: Vec<MoveToChild>,
    pub author: String,
    pub date: String,
}

impl Default for MoveTo {
    fn default() -> MoveTo {
        MoveTo {
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            children: vec![],
        }
    }
}

impl MoveTo {
    pub fn new(run: Run) -> MoveTo {
        Self {
            children: vec![MoveToChild::Run(Box::new(run))],
            ..Default::default()
        }
    }

    pub fn new_with_empty() -> MoveTo {
        Self {
            ..Default::default()
        }
    }

    pub fn add_run(mut self, run: Run) -> MoveTo {
        self.children.push(MoveToChild::Run(Box::new(run)));
        self
    }

    pub fn add_delete(mut self, del: Delete) -> MoveTo {
        self.children.push(MoveToChild::Delete(del));
        self
    }

    pub fn add_child(mut self, c: MoveToChild) -> MoveTo {
        self.children.push(c);
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> Self {
        self.children
            .push(MoveToChild::CommentStart(Box::new(CommentRangeStart::new(
                comment,
            ))));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> Self {
        self.children
            .push(MoveToChild::CommentEnd(CommentRangeEnd::new(id)));
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> MoveTo {
        self.author = escape::escape_owned(author.into());
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> MoveTo {
        self.date = date.into();
        self
    }
}

impl HistoryId for MoveTo {}

impl BuildXML for MoveTo {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = self.generate();
        XMLBuilder::from(stream)
            .open_move_to(id.as_ref(), &self.author, &self.date)?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}

