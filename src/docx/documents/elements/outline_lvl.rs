use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineLvl {
    pub v: usize,
}

impl OutlineLvl {
    pub fn new(v: usize) -> OutlineLvl {
        assert!(v < 10, "outline level should be less than 10");
        OutlineLvl { v }
    }
}

impl BuildXML for OutlineLvl {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .outline_lvl(self.v)?
            // .close()?
            .into_inner()
    }
}
