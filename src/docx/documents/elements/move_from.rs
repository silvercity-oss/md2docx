use std::io::Write;

use crate::docx::xml_builder::*;
use crate::docx::{documents::*, escape};

#[derive(Debug, Clone, PartialEq)]
pub struct MoveFrom {
    pub author: String,
    pub date: String,
    pub children: Vec<MoveFromChild>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MoveFromChild {
    Run(Box<Run>),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
}

impl Default for MoveFrom {
    fn default() -> MoveFrom {
        MoveFrom {
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            children: vec![],
        }
    }
}

impl MoveFrom {
    pub fn new() -> MoveFrom {
        Self {
            children: vec![],
            ..Default::default()
        }
    }

    pub fn add_run(mut self, run: Run) -> MoveFrom {
        self.children.push(MoveFromChild::Run(Box::new(run)));
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> MoveFrom {
        self.children.push(MoveFromChild::CommentStart(Box::new(
            CommentRangeStart::new(comment),
        )));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> MoveFrom {
        self.children
            .push(MoveFromChild::CommentEnd(CommentRangeEnd::new(id)));
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> MoveFrom {
        self.author = escape::escape_owned(author.into());
        self
    }

    pub fn date(mut self, date: impl Into<String>) -> MoveFrom {
        self.date = date.into();
        self
    }
}

impl HistoryId for MoveFrom {}

impl BuildXML for MoveFrom {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = self.generate();
        XMLBuilder::from(stream)
            .open_move_from(id.as_ref(), &self.author, &self.date)?
            .apply_each(&self.children, |ch, b| match ch {
                MoveFromChild::Run(t) => b.add_child(&**t),
                MoveFromChild::CommentStart(c) => b.add_child(&**c),
                MoveFromChild::CommentEnd(c) => b.add_child(c),
            })?
            .close()?
            .into_inner()
    }
}

