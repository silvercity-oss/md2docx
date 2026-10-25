use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Underline {
    val: String,
}

impl Underline {
    pub fn new(val: impl Into<String>) -> Underline {
        Underline { val: val.into() }
    }
}

impl BuildXML for Underline {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).underline(&self.val)?.into_inner()
    }
}
