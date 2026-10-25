use std::io::Write;

use crate::docx::{xml_builder::XMLBuilder, BuildXML};

// use crate::docx::documents::BuildXML;
// use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Caps {
    val: bool,
}

impl Caps {}

impl Default for Caps {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for Caps {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).caps(&self.val.to_string())?.into_inner()
    }
}
