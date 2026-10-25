use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
// use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredDataTag {
    pub children: Vec<StructuredDataTagChild>,
    pub property: StructuredDataTagProperty,
    pub has_numbering: bool,
}

impl Default for StructuredDataTag {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            property: StructuredDataTagProperty::new(),
            has_numbering: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StructuredDataTagChild {
    Run(Box<Run>),
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    BookmarkStart(BookmarkStart),
    BookmarkEnd(BookmarkEnd),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
    StructuredDataTag(Box<StructuredDataTag>),
}

impl BuildXML for StructuredDataTagChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            StructuredDataTagChild::Run(v) => v.build_to(stream),
            StructuredDataTagChild::Paragraph(v) => v.build_to(stream),
            StructuredDataTagChild::Table(v) => v.build_to(stream),
            StructuredDataTagChild::BookmarkStart(v) => v.build_to(stream),
            StructuredDataTagChild::BookmarkEnd(v) => v.build_to(stream),
            StructuredDataTagChild::CommentStart(v) => v.build_to(stream),
            StructuredDataTagChild::CommentEnd(v) => v.build_to(stream),
            StructuredDataTagChild::StructuredDataTag(v) => v.build_to(stream),
        }
    }
}

impl StructuredDataTag {}

impl BuildXML for StructuredDataTag {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_structured_tag()?
            .add_child(&self.property)?
            .open_structured_tag_content()?
            .add_children(&self.children)?
            .close()?
            .close()?
            .into_inner()
    }
}
