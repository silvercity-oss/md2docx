use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub has_numbering: bool,
    pub children: Vec<HeaderChild>,
}

impl Header {}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    StructuredDataTag(Box<StructuredDataTag>),
}

impl BuildXML for Header {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_header()?
            .apply_each(&self.children, |c, b| match c {
                HeaderChild::Paragraph(p) => b.add_child(&p),
                HeaderChild::Table(t) => b.add_child(&t),
                HeaderChild::StructuredDataTag(t) => b.add_child(&t),
            })?
            .close()?
            .into_inner()
    }
}
