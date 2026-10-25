use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Italic {
    val: bool,
}

impl Italic {
    pub fn new() -> Italic {
        Default::default()
    }
}

impl Default for Italic {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for Italic {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        if self.val {
            XMLBuilder::from(stream).i()?.into_inner()
        } else {
            XMLBuilder::from(stream).disable_italic()?.into_inner()
        }
    }
}
