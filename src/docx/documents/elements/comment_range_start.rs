use std::io::Write;

use super::Comment;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CommentRangeStart {
    pub id: usize,
    pub comment: Comment,
}

impl CommentRangeStart {
    pub fn new(comment: Comment) -> CommentRangeStart {
        CommentRangeStart {
            id: comment.id(),
            comment,
        }
    }

    pub(crate) fn comment(&mut self, comment: Comment) {
        self.comment = comment;
    }

    pub(crate) fn get_comment_ref(&self) -> &Comment {
        &self.comment
    }

    pub(crate) fn get_id(&self) -> usize {
        self.id
    }
}

impl BuildXML for CommentRangeStart {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .comment_range_start(&self.id.to_string())?
            .into_inner()
    }
}

