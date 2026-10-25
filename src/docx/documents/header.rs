use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub has_numbering: bool,
    pub children: Vec<HeaderChild>,
}

impl Header {
    pub fn new() -> Header {
        Default::default()
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        if p.has_numbering {
            self.has_numbering = true
        }
        self.children.push(HeaderChild::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(HeaderChild::Table(Box::new(t)));
        self
    }

    /// reader only
    pub(crate) fn add_structured_data_tag(mut self, t: StructuredDataTag) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children
            .push(HeaderChild::StructuredDataTag(Box::new(t)));
        self
    }
}

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

