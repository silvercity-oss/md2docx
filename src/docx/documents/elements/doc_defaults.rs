use std::io::Write;

use crate::docx::{documents::BuildXML, RunProperty};
use crate::docx::{xml_builder::*, LineSpacing, ParagraphProperty, ParagraphPropertyDefault};

use super::run_property_default::*;
use super::RunFonts;

#[derive(Debug, Clone, PartialEq)]
pub struct DocDefaults {
    run_property_default: RunPropertyDefault,
    paragraph_property_default: ParagraphPropertyDefault,
}

impl DocDefaults {
    pub fn new() -> DocDefaults {
        Default::default()
    }

    pub fn size(mut self, size: usize) -> Self {
        self.run_property_default = self.run_property_default.size(size);
        self
    }

    pub fn spacing(mut self, spacing: i32) -> Self {
        self.run_property_default = self.run_property_default.spacing(spacing);
        self
    }

    pub fn fonts(mut self, font: RunFonts) -> Self {
        self.run_property_default = self.run_property_default.fonts(font);
        self
    }

    pub fn line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.paragraph_property_default = self.paragraph_property_default.line_spacing(spacing);
        self
    }

    pub(crate) fn run_property(mut self, p: RunProperty) -> Self {
        self.run_property_default = self.run_property_default.run_property(p);
        self
    }

    pub(crate) fn paragraph_property(mut self, p: ParagraphProperty) -> Self {
        self.paragraph_property_default = self.paragraph_property_default.paragraph_property(p);
        self
    }
}

impl Default for DocDefaults {
    fn default() -> Self {
        let run_property_default = RunPropertyDefault::new();
        let paragraph_property_default = ParagraphPropertyDefault::new();
        DocDefaults {
            run_property_default,
            paragraph_property_default,
        }
    }
}

impl BuildXML for DocDefaults {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_doc_defaults()?
            .add_child(&self.run_property_default)?
            .add_child(&self.paragraph_property_default)?
            .close()?
            .into_inner()
    }
}

