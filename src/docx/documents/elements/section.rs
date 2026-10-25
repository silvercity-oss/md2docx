use super::*;
// use crate::docx::create_header_rid;
use crate::docx::documents::BuildXML;
use crate::docx::macros::delegate_to_field;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use crate::docx::Footer;
use crate::docx::Header;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum SectionChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    BookmarkStart(BookmarkStart),
    BookmarkEnd(BookmarkEnd),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
    StructuredDataTag(Box<StructuredDataTag>),
    TableOfContents(Box<TableOfContents>),
}

impl BuildXML for SectionChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            SectionChild::Paragraph(v) => v.build_to(stream),
            SectionChild::Table(v) => v.build_to(stream),
            SectionChild::BookmarkStart(v) => v.build_to(stream),
            SectionChild::BookmarkEnd(v) => v.build_to(stream),
            SectionChild::CommentStart(v) => v.build_to(stream),
            SectionChild::CommentEnd(v) => v.build_to(stream),
            SectionChild::StructuredDataTag(v) => v.build_to(stream),
            SectionChild::TableOfContents(v) => v.build_to(stream),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub(crate) property: SectionProperty,
    pub(crate) children: Vec<SectionChild>,
    pub(crate) has_numbering: bool,
    pub(crate) temp_header: Option<Header>,
    pub(crate) temp_first_header: Option<Header>,
    pub(crate) temp_even_header: Option<Header>,
    pub(crate) temp_footer: Option<Footer>,
    pub(crate) temp_first_footer: Option<Footer>,
    pub(crate) temp_even_footer: Option<Footer>,
}

impl Section {
    delegate_to_field! {
        property =>
        page_size(size: PageSize) -> Self,
        page_margin(margin: PageMargin) -> Self,
        // header(h: Header, rid: &str) -> Self,
        // first_header(h: Header, rid: &str) -> Self,
        // first_header_without_title_pg(h: Header, rid: &str) -> Self,
        // even_header(h: Header, rid: &str) -> Self,
        // footer(h: Footer, rid: &str) -> Self,
        // first_footer(h: Footer, rid: &str) -> Self,
        // first_footer_without_title_pg(h: Footer, rid: &str) -> Self,
        // even_footer(h: Footer, rid: &str) -> Self,
    }
}

impl Default for Section {
    fn default() -> Self {
        Self {
            property: SectionProperty::new(),
            children: vec![],
            has_numbering: false,
            temp_header: None,
            temp_first_header: None,
            temp_even_header: None,
            temp_footer: None,
            temp_first_footer: None,
            temp_even_footer: None,
        }
    }
}

impl BuildXML for Section {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = crate::docx::generate_para_id();
        XMLBuilder::from(stream)
            .add_children(&self.children)?
            .open_paragraph(&id)?
            .open_paragraph_property()?
            .add_child(&self.property)?
            .close()?
            .close()?
            .into_inner()
    }
}
