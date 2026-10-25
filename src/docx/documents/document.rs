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

    pub fn add_bookmark_start(mut self, id: usize, name: impl Into<String>) -> Self {
        self.children
            .push(DocumentChild::BookmarkStart(BookmarkStart::new(id, name)));
        self
    }

    pub fn add_bookmark_end(mut self, id: usize) -> Self {
        self.children
            .push(DocumentChild::BookmarkEnd(BookmarkEnd::new(id)));
        self
    }

    pub fn add_comment_start(mut self, comment: Comment) -> Self {
        self.children.push(DocumentChild::CommentStart(Box::new(
            CommentRangeStart::new(comment),
        )));
        self
    }

    pub fn add_comment_end(mut self, id: usize) -> Self {
        self.children
            .push(DocumentChild::CommentEnd(CommentRangeEnd::new(id)));
        self
    }

    pub fn add_section(mut self, sec: Section) -> Self {
        self.children.push(DocumentChild::Section(Box::new(sec)));
        self
    }

    pub fn title_pg(mut self) -> Self {
        self.section_property = self.section_property.title_pg();
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

    pub fn page_orient(mut self, o: crate::docx::types::PageOrientationType) -> Self {
        self.section_property = self.section_property.page_orient(o);
        self
    }

    pub fn doc_grid(mut self, doc_grid: DocGrid) -> Self {
        self.section_property = self.section_property.doc_grid(doc_grid);
        self
    }

    pub fn default_section_property(mut self, property: SectionProperty) -> Self {
        self.section_property = property;
        self
    }

    pub fn header(mut self, h: Header, rid: &str) -> Self {
        self.section_property = self.section_property.header(h, rid);
        self
    }

    pub fn first_header(mut self, h: Header, rid: &str) -> Self {
        self.section_property = self.section_property.first_header(h, rid);
        self
    }

    pub(crate) fn first_header_without_title_pg(mut self, h: Header, rid: &str) -> Self {
        self.section_property = self.section_property.first_header_without_title_pg(h, rid);
        self
    }

    pub fn even_header(mut self, h: Header, rid: &str) -> Self {
        self.section_property = self.section_property.even_header(h, rid);
        self
    }

    pub fn footer(mut self, h: Footer, rid: &str) -> Self {
        self.section_property = self.section_property.footer(h, rid);
        self
    }

    pub fn first_footer(mut self, h: Footer, rid: &str) -> Self {
        self.section_property = self.section_property.first_footer(h, rid);
        self
    }

    pub(crate) fn first_footer_without_title_pg(mut self, h: Footer, rid: &str) -> Self {
        self.section_property = self.section_property.first_footer_without_title_pg(h, rid);
        self
    }

    pub fn even_footer(mut self, h: Footer, rid: &str) -> Self {
        self.section_property = self.section_property.even_footer(h, rid);
        self
    }

    pub fn add_structured_data_tag(mut self, t: StructuredDataTag) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children
            .push(DocumentChild::StructuredDataTag(Box::new(t)));
        self
    }

    pub fn add_table_of_contents(mut self, t: TableOfContents) -> Self {
        self.children
            .push(DocumentChild::TableOfContents(Box::new(t)));
        self
    }

    pub fn columns(mut self, col: usize) -> Self {
        self.section_property.columns = col;
        self
    }

    pub fn text_direction(mut self, direction: String) -> Self {
        self.section_property.text_direction = direction;
        self
    }

    pub fn page_num_type(mut self, p: PageNumType) -> Self {
        self.section_property = self.section_property.page_num_type(p);
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

