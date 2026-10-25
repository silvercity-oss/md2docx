use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::XMLBuilder;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InstrNUMPAGES {}

impl InstrNUMPAGES {}

impl BuildXML for InstrNUMPAGES {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).plain_text("NUMPAGES")?.into_inner()
    }
}
