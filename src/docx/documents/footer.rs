use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Footer {
    pub has_numbering: bool,
    pub children: Vec<FooterChild>,
}

impl Footer {
    pub fn new() -> Footer {
        Default::default()
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        if p.has_numbering {
            self.has_numbering = true
        }
        self.children.push(FooterChild::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(FooterChild::Table(Box::new(t)));
        self
    }

    /// reader only
    pub(crate) fn add_structured_data_tag(mut self, t: StructuredDataTag) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children
            .push(FooterChild::StructuredDataTag(Box::new(t)));
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FooterChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    StructuredDataTag(Box<StructuredDataTag>),
}

impl BuildXML for Footer {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_footer()?
            .apply_each(&self.children, |c, b| match c {
                FooterChild::Paragraph(p) => b.add_child(&p),
                FooterChild::Table(t) => b.add_child(&t),
                FooterChild::StructuredDataTag(t) => b.add_child(&t),
            })?
            .close()?
            .into_inner()
    }
}

