use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct FooterReference {
    pub footer_type: String,
    pub id: String,
}

impl FooterReference {
    pub fn new(t: impl Into<String>, id: impl Into<String>) -> FooterReference {
        FooterReference {
            footer_type: t.into(),
            id: id.into(),
        }
    }
}

impl BuildXML for FooterReference {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .footer_reference(&self.footer_type, &self.id)?
            .into_inner()
    }
}
