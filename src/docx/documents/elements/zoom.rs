use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Zoom {
    val: usize,
}

impl Zoom {
    pub fn new(val: usize) -> Zoom {
        Zoom { val }
    }
}

impl BuildXML for Zoom {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).zoom(&format!("{}", self.val))?.into_inner()
    }
}
