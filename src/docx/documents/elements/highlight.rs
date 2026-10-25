use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    val: String,
}

impl Highlight {
    pub fn new(val: impl Into<String>) -> Highlight {
        Highlight { val: val.into() }
    }
}

impl BuildXML for Highlight {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).highlight(&self.val)?.into_inner()
    }
}

