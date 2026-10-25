use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

// i.e.    <w15:commentEx w15:paraId="00000001" w15:paraIdParent="57D1BD7C" w15:done="0"/>
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommentsExtended {
    pub children: Vec<CommentExtended>,
}

impl CommentsExtended {
    pub fn new() -> CommentsExtended {
        Default::default()
    }

    pub fn add_comments_extended(&mut self, c: Vec<CommentExtended>) {
        self.children = c;
    }
}

impl BuildXML for CommentsExtended {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_comments_extended()?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}
