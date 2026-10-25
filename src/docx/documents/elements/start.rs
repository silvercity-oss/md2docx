use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Start {
    val: usize,
}

impl Start {
    pub fn new(val: usize) -> Start {
        Start { val }
    }
}

impl BuildXML for Start {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).start(self.val)?.into_inner()
    }
}
