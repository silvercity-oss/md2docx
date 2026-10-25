use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub children: Vec<DocumentChild>,
    pub section_property: SectionProperty,
    pub has_numbering: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    BookmarkStart(BookmarkStart),
    BookmarkEnd(BookmarkEnd),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
    StructuredDataTag(Box<StructuredDataTag>),
    TableOfContents(Box<TableOfContents>),
    Section(Box<Section>),
}

impl Default for Document {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            section_property: SectionProperty::new(),
            has_numbering: false,
        }
    }
}

impl Document {
    pub fn new() -> Document {
        Default::default()
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        if p.has_numbering {
            self.has_numbering = true
        }
        self.children.push(DocumentChild::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(DocumentChild::Table(Box::new(t)));
        self
    }

    pub fn page_size(mut self, size: PageSize) -> Self {
        self.section_property = self.section_property.page_size(size);
        self
    }

    pub fn page_margin(mut self, margin: crate::docx::types::PageMargin) -> Self {
        self.section_property = self.section_property.page_margin(margin);
        self
    }

    /// Iterates headers in relationship creation order.
    ///
    /// Relationship IDs are assigned to the document property first and then
    /// to section children in insertion order. Preserving that order avoids a
    /// sort and keeps double-digit IDs aligned with numbered package parts.
    pub(crate) fn headers(&self) -> impl Iterator<Item = &(String, Header)> {
        self.section_property.headers().chain(
            self.children
                .iter()
                .filter_map(|child| match child {
                    DocumentChild::Section(section) => Some(section),
                    _ => None,
                })
                .flat_map(|section| section.property.headers()),
        )
    }

    /// Iterates mutable headers in the same order as [`Self::headers`].
    pub(crate) fn headers_mut(&mut self) -> impl Iterator<Item = &mut (String, Header)> {
        self.section_property.headers_mut().chain(
            self.children
                .iter_mut()
                .filter_map(|child| match child {
                    DocumentChild::Section(section) => Some(section),
                    _ => None,
                })
                .flat_map(|section| section.property.headers_mut()),
        )
    }

    /// Iterates footers in relationship creation order.
    pub(crate) fn footers(&self) -> impl Iterator<Item = &(String, Footer)> {
        self.section_property.footers().chain(
            self.children
                .iter()
                .filter_map(|child| match child {
                    DocumentChild::Section(section) => Some(section),
                    _ => None,
                })
                .flat_map(|section| section.property.footers()),
        )
    }

    /// Iterates mutable footers in the same order as [`Self::footers`].
    pub(crate) fn footers_mut(&mut self) -> impl Iterator<Item = &mut (String, Footer)> {
        self.section_property.footers_mut().chain(
            self.children
                .iter_mut()
                .filter_map(|child| match child {
                    DocumentChild::Section(section) => Some(section),
                    _ => None,
                })
                .flat_map(|section| section.property.footers_mut()),
        )
    }
}

impl BuildXML for DocumentChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            DocumentChild::Paragraph(v) => v.build_to(stream),
            DocumentChild::Table(v) => v.build_to(stream),
            DocumentChild::BookmarkStart(v) => v.build_to(stream),
            DocumentChild::BookmarkEnd(v) => v.build_to(stream),
            DocumentChild::CommentStart(v) => v.build_to(stream),
            DocumentChild::CommentEnd(v) => v.build_to(stream),
            DocumentChild::StructuredDataTag(v) => v.build_to(stream),
            DocumentChild::TableOfContents(v) => v.build_to(stream),
            DocumentChild::Section(v) => v.build_to(stream),
        }
    }
}

impl BuildXML for Document {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_document()?
            .open_body()?
            .add_children(&self.children)?
            .add_child(&self.section_property)?
            .close()?
            .close()?
            .into_inner()
    }
}
