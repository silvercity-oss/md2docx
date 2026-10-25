use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct NumberingId {
    pub id: usize,
}

impl NumberingId {
    pub fn new(id: usize) -> NumberingId {
        NumberingId { id }
    }
}

impl BuildXML for NumberingId {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).num_id(self.id)?.into_inner()
    }
}

