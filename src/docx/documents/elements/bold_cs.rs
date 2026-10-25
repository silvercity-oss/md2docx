use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BoldCs {
    val: bool,
}

impl BoldCs {
    pub fn new() -> BoldCs {
        Default::default()
    }
    pub fn disable(mut self) -> BoldCs {
        self.val = false;
        self
    }
}

impl Default for BoldCs {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for BoldCs {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).b_cs()?.into_inner()
    }
}
