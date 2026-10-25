use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone)]
pub struct CustomItemProperty {
    id: String,
}

impl CustomItemProperty {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl BuildXML for CustomItemProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(false))?
            .open_data_store_item(
                "http://schemas.openxmlformats.org/officeDocument/2006/customXml",
                &format!("{{{}}}", self.id),
            )?
            .open_data_store_schema_refs()?
            .close()?
            .close()?
            .into_inner()
    }
}
