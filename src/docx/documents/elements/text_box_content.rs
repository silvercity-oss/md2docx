use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextBoxContent {
    pub children: Vec<TextBoxContentChild>,
    pub has_numbering: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextBoxContentChild {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

impl TextBoxContent {
    pub fn new() -> TextBoxContent {
        Default::default()
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> Self {
        if p.has_numbering {
            self.has_numbering = true
        }
        self.children
            .push(TextBoxContentChild::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> Self {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(TextBoxContentChild::Table(Box::new(t)));
        self
    }
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

impl BuildXML for TextBoxContent {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_text_box_content()?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}

