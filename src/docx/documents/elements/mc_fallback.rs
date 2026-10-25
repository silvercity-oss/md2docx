use crate::docx::documents::BuildXML;
use std::io::Write;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct McFallback {}

impl McFallback {
    pub fn new() -> McFallback {
        Default::default()
    }
}

impl BuildXML for McFallback {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        // Ignore for now
        Ok(stream)
    }
}
