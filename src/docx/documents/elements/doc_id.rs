use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct DocId {
    id: String,
}

impl DocId {}

impl BuildXML for DocId {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = format!("{{{}}}", self.id);
        XMLBuilder::from(stream).doc_id(&id)?.into_inner()
    }
}
