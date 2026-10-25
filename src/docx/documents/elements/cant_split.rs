use std::io::Write;

use crate::docx::{xml_builder::XMLBuilder, BuildXML};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CantSplit {}
impl BuildXML for CantSplit {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).cant_split()?.into_inner()
    }
}

