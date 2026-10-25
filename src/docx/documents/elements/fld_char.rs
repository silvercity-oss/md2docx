use std::io::Write;

use crate::docx::documents::*;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldChar {
    pub field_char_type: FieldCharType,
    pub dirty: bool,
}

impl FieldChar {
    pub fn new(t: FieldCharType) -> Self {
        Self {
            field_char_type: t,
            dirty: false,
        }
    }

    pub fn dirty(mut self) -> Self {
        self.dirty = true;
        self
    }
}

impl BuildXML for FieldChar {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .field_character(
                &format!("{}", self.field_char_type),
                &format!("{}", self.dirty),
            )?
            .into_inner()
    }
}

