use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CustomItemRels {
    custom_item_count: usize,
}

impl CustomItemRels {}

impl BuildXML for CustomItemRels {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_relationships("http://schemas.openxmlformats.org/package/2006/relationships")?
            .apply_each(0..self.custom_item_count, |id, b| {
                b.relationship(
                    &format!("rId{}", id + 1),
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps",
                    &format!("itemProps{}.xml", id + 1),
                )
            })?
            .close()?
            .into_inner()
    }
}
