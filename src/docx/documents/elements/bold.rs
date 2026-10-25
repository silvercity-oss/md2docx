use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Bold {
    val: bool,
}

impl Bold {
    pub fn new() -> Bold {
        Default::default()
    }

    pub fn disable(mut self) -> Bold {
        self.val = false;
        self
    }
}

impl Default for Bold {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for Bold {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        if self.val {
            XMLBuilder::from(stream).b()?.into_inner()
        } else {
            XMLBuilder::from(stream).disable_bold()?.into_inner()
        }
    }
}
