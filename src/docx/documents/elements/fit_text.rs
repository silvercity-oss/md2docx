use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct FitText {
    pub val: usize,
    pub id: Option<u32>,
}

impl FitText {}

impl BuildXML for FitText {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).fit_text(self.val, self.id)?.into_inner()
    }
}
