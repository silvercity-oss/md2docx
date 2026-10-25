use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SoftHyphen {}

impl Default for SoftHyphen {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftHyphen {
    pub fn new() -> SoftHyphen {
        SoftHyphen {}
    }
}

impl BuildXML for SoftHyphen {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).soft_hyphen()?.into_inner()
    }
}
