use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Break {
    break_type: BreakType,
}

impl Break {
    pub fn new(t: BreakType) -> Break {
        Break { break_type: t }
    }
}

impl BuildXML for Break {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .br(&self.break_type.to_string())?
            .into_inner()
    }
}

