use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::XMLBuilder;
use std::io::Write;

/// An opaque custom XML part, written verbatim.
#[derive(Debug, Clone)]
pub struct CustomItem(String);

impl CustomItem {
    pub fn new(xml: impl Into<String>) -> Self {
        CustomItem(xml.into())
    }
}

impl BuildXML for CustomItem {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let mut b = XMLBuilder::from(stream);
        write!(b.inner_mut()?, "{}", self.0)?;
        b.into_inner()
    }
}
