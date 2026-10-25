use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Sz {
    val: usize,
}

impl Sz {
    pub fn new(val: usize) -> Sz {
        Sz { val }
    }
}

impl BuildXML for Sz {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).sz(self.val)?.into_inner()
    }
}

