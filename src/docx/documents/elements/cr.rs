use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CarriageReturn {}

impl Default for CarriageReturn {
    fn default() -> Self {
        Self::new()
    }
}

impl CarriageReturn {
    pub fn new() -> CarriageReturn {
        CarriageReturn {}
    }
}

impl BuildXML for CarriageReturn {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).cr()?.into_inner()
    }
}

