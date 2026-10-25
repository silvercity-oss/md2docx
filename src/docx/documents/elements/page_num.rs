use std::io::Write;

use crate::docx::documents::*;
use crate::docx::types::*;

#[derive(Debug, Clone, PartialEq)]
pub struct PageNum {
    pub instr: InstrPAGE,
}

impl Default for PageNum {
    fn default() -> Self {
        Self {
            instr: InstrPAGE {},
        }
    }
}

impl PageNum {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BuildXML for PageNum {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        Run::new()
            .add_field_char(FieldCharType::Begin, false)
            .add_instr_text(InstrText::PAGE(self.instr.clone()))
            .add_field_char(FieldCharType::Separate, false)
            .add_text("1")
            .add_field_char(FieldCharType::End, false)
            .build_to(stream)
    }
}

