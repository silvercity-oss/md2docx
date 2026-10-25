use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct HeaderReference {
    pub header_type: String,
    pub id: String,
}

impl Default for HeaderReference {
    fn default() -> HeaderReference {
        HeaderReference {
            header_type: "default".to_owned(),
            id: "rId4".to_owned(),
        }
    }
}

impl HeaderReference {}

impl BuildXML for HeaderReference {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .header_reference(&self.header_type, &self.id)?
            .into_inner()
    }
}
