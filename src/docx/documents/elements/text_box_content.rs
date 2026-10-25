use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub enum TextBoxContentChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

impl BuildXML for TextBoxContentChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            TextBoxContentChild::Paragraph(p) => p.build_to(stream),
            TextBoxContentChild::Table(t) => t.build_to(stream),
        }
    }
}
