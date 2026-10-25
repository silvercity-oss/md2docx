use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataBinding {
    pub xpath: Option<String>,
    pub prefix_mappings: Option<String>,
    pub store_item_id: Option<String>,
}

impl DataBinding {}

impl BuildXML for DataBinding {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .data_binding(
                self.xpath.as_ref(),
                self.prefix_mappings.as_ref(),
                self.store_item_id.as_ref(),
            )?
            .into_inner()
    }
}
