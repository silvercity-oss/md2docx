use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ItalicCs {
    val: bool,
}

impl ItalicCs {
    pub fn new() -> ItalicCs {
        Default::default()
    }
}

impl Default for ItalicCs {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for ItalicCs {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).i_cs()?.into_inner()
    }
}
