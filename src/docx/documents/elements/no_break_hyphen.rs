use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct NoBreakHyphen {}

impl Default for NoBreakHyphen {
    fn default() -> Self {
        Self::new()
    }
}

impl NoBreakHyphen {
    pub fn new() -> NoBreakHyphen {
        NoBreakHyphen {}
    }
}

impl BuildXML for NoBreakHyphen {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).no_break_hyphen()?.into_inner()
    }
}

