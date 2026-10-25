use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Strike {
    pub val: bool,
}

impl Strike {
    pub fn new() -> Strike {
        Default::default()
    }
}

impl Default for Strike {
    fn default() -> Self {
        Self { val: true }
    }
}

impl BuildXML for Strike {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        if self.val {
            XMLBuilder::from(stream).strike()?.into_inner()
        } else {
            XMLBuilder::from(stream).disable_strike()?.into_inner()
        }
    }
}
