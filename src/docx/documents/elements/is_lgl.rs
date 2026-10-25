use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IsLgl {}

impl IsLgl {
    pub fn new() -> IsLgl {
        IsLgl {}
    }
}

impl BuildXML for IsLgl {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).is_lgl()?.into_inner()
    }
}

