use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Dstrike {
    pub val: bool,
}

impl Dstrike {}

impl Default for Dstrike {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for Dstrike {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        if self.val {
            XMLBuilder::from(stream).dstrike()?.into_inner()
        } else {
            XMLBuilder::from(stream).disable_dstrike()?.into_inner()
        }
    }
}
