use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageNumType {
    pub start: Option<u32>,
    pub chap_style: Option<String>,
}

impl PageNumType {}

impl BuildXML for PageNumType {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .page_num_type(self.start, self.chap_style.clone())?
            .into_inner()
    }
}
