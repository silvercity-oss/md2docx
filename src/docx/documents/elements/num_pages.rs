use std::io::Write;

use crate::docx::documents::*;
use crate::docx::types::*;

#[derive(Debug, Clone, PartialEq)]
pub struct NumPages {
    pub instr: InstrNUMPAGES,
}

impl Default for NumPages {
    fn default() -> Self {
        Self {
            instr: InstrNUMPAGES {},
        }
    }
}

impl NumPages {}

impl BuildXML for NumPages {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        Run::new()
            .add_field_char(FieldCharType::Begin, false)
            .add_instr_text(InstrText::NUMPAGES(self.instr.clone()))
            .add_field_char(FieldCharType::Separate, false)
            .add_text("1")
            .add_field_char(FieldCharType::End, false)
            .build_to(stream)
    }
}
