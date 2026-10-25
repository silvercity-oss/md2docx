use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct LevelJc {
    val: String,
}

impl LevelJc {
    pub fn new(val: impl Into<String>) -> Self {
        Self { val: val.into() }
    }
}

impl BuildXML for LevelJc {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).level_justification(&self.val)?.into_inner()
    }
}
