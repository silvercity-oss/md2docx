use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct SzCs {
    val: usize,
}

impl SzCs {
    pub fn new(val: usize) -> SzCs {
        SzCs { val }
    }
}

impl BuildXML for SzCs {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).sz_cs(self.val)?.into_inner()
    }
}

