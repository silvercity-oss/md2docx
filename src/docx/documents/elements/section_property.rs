use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use crate::docx::{Footer, Header};
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct SectionProperty {
    pub page_size: PageSize,
    pub page_margin: PageMargin,
    pub columns: usize,
    pub space: usize,
    pub title_pg: bool,
    pub text_direction: String,
    pub doc_grid: Option<DocGrid>,
    pub header_reference: Option<HeaderReference>,
    pub header: Option<(String, Header)>,
    pub first_header_reference: Option<HeaderReference>,
    pub first_header: Option<(String, Header)>,
    pub even_header_reference: Option<HeaderReference>,
    pub even_header: Option<(String, Header)>,
    pub footer_reference: Option<FooterReference>,
    pub footer: Option<(String, Footer)>,
    pub first_footer_reference: Option<FooterReference>,
    pub first_footer: Option<(String, Footer)>,
    pub even_footer_reference: Option<FooterReference>,
    pub even_footer: Option<(String, Footer)>,
    pub section_type: Option<SectionType>,
    pub page_num_type: Option<PageNumType>,
}

impl SectionProperty {
    pub fn new() -> SectionProperty {
        Default::default()
    }

    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = size;
        self
    }

    pub fn page_margin(mut self, margin: PageMargin) -> Self {
        self.page_margin = margin;
        self
    }

    pub fn page_orient(mut self, o: PageOrientationType) -> Self {
        self.page_size = self.page_size.orient(o);
        self
    }

    pub fn doc_grid(mut self, doc_grid: DocGrid) -> Self {
        self.doc_grid = Some(doc_grid);
        self
    }

    pub fn text_direction(mut self, direction: String) -> Self {
        self.text_direction = direction;
        self
    }

    pub fn title_pg(mut self) -> Self {
        self.title_pg = true;
        self
    }

    pub fn header(mut self, h: Header, rid: &str) -> Self {
        self.header_reference = Some(HeaderReference::new("default", rid));
        self.header = Some((rid.to_string(), h));
        self
    }

    pub fn first_header(mut self, h: Header, rid: &str) -> Self {
        self.first_header_reference = Some(HeaderReference::new("first", rid));
        self.first_header = Some((rid.to_string(), h));
        self.title_pg = true;
        self
    }

    pub fn first_header_without_title_pg(mut self, h: Header, rid: &str) -> Self {
        self.first_header_reference = Some(HeaderReference::new("first", rid));
        self.first_header = Some((rid.to_string(), h));
        self
    }

    pub fn even_header(mut self, h: Header, rid: &str) -> Self {
        self.even_header_reference = Some(HeaderReference::new("even", rid));
        self.even_header = Some((rid.to_string(), h));
        self
    }

    pub fn footer(mut self, h: Footer, rid: &str) -> Self {
        self.footer_reference = Some(FooterReference::new("default", rid));
        self.footer = Some((rid.to_string(), h));
        self
    }

    pub fn first_footer(mut self, h: Footer, rid: &str) -> Self {
        self.first_footer_reference = Some(FooterReference::new("first", rid));
        self.first_footer = Some((rid.to_string(), h));
        self.title_pg = true;
        self
    }

    pub fn first_footer_without_title_pg(mut self, h: Footer, rid: &str) -> Self {
        self.first_footer_reference = Some(FooterReference::new("first", rid));
        self.first_footer = Some((rid.to_string(), h));
        self
    }

    pub fn even_footer(mut self, h: Footer, rid: &str) -> Self {
        self.even_footer_reference = Some(FooterReference::new("even", rid));
        self.even_footer = Some((rid.to_string(), h));
        self
    }

    /// Returns the default, first, and even headers that are present.
    pub fn get_headers(&self) -> Vec<&(String, Header)> {
        self.headers().collect()
    }

    /// Returns the default, first, and even footers that are present.
    pub fn get_footers(&self) -> Vec<&(String, Footer)> {
        self.footers().collect()
    }

    /// Iterates headers without allocating the compatibility `Vec` returned by
    /// [`Self::get_headers`].
    pub(crate) fn headers(&self) -> impl Iterator<Item = &(String, Header)> {
        [&self.header, &self.first_header, &self.even_header]
            .into_iter()
            .filter_map(Option::as_ref)
    }

    /// Iterates mutable headers so package metadata can be collected in place.
    pub(crate) fn headers_mut(&mut self) -> impl Iterator<Item = &mut (String, Header)> {
        [
            &mut self.header,
            &mut self.first_header,
            &mut self.even_header,
        ]
        .into_iter()
        .filter_map(Option::as_mut)
    }

    /// Iterates footers without allocating the compatibility `Vec` returned by
    /// [`Self::get_footers`].
    pub(crate) fn footers(&self) -> impl Iterator<Item = &(String, Footer)> {
        [&self.footer, &self.first_footer, &self.even_footer]
            .into_iter()
            .filter_map(Option::as_ref)
    }

    /// Iterates mutable footers so package metadata can be collected in place.
    pub(crate) fn footers_mut(&mut self) -> impl Iterator<Item = &mut (String, Footer)> {
        [
            &mut self.footer,
            &mut self.first_footer,
            &mut self.even_footer,
        ]
        .into_iter()
        .filter_map(Option::as_mut)
    }

    pub fn page_num_type(mut self, h: PageNumType) -> Self {
        self.page_num_type = Some(h);
        self
    }
}

impl Default for SectionProperty {
    fn default() -> Self {
        Self {
            page_size: PageSize::new(),
            page_margin: PageMargin::new(),
            columns: 1,
            space: 425,
            title_pg: false,
            text_direction: "lrTb".to_string(),
            doc_grid: None,
            // headers
            header_reference: None,
            header: None,
            first_header_reference: None,
            first_header: None,
            even_header_reference: None,
            even_header: None,
            // footers
            footer_reference: None,
            footer: None,
            first_footer_reference: None,
            first_footer: None,
            even_footer_reference: None,
            even_footer: None,
            section_type: None,
            page_num_type: None,
        }
    }
}

impl BuildXML for SectionProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_section_property()?
            .add_child(&self.page_size)?
            .add_child(&self.page_margin)?
            .columns(&format!("{}", self.space), &format!("{}", self.columns))?
            .add_optional_child(&self.doc_grid)?
            .add_optional_child(&self.header_reference)?
            .add_optional_child(&self.first_header_reference)?
            .add_optional_child(&self.even_header_reference)?
            .add_optional_child(&self.footer_reference)?
            .add_optional_child(&self.first_footer_reference)?
            .add_optional_child(&self.even_footer_reference)?
            .add_optional_child(&self.page_num_type)?
            .apply_if(self.text_direction != "lrTb", |b| {
                b.text_direction(&self.text_direction)
            })?
            .apply_opt(self.section_type, |t, b| b.type_tag(&t.to_string()))?
            .apply_if(self.title_pg, |b| b.title_pg())?
            .close()?
            .into_inner()
    }
}

