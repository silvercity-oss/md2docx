use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct LevelText {
    val: String,
}

impl LevelText {
    pub fn new(val: impl Into<String>) -> Self {
        Self { val: val.into() }
    }
}

impl BuildXML for LevelText {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).level_text(&self.val)?.into_inner()
    }
}

