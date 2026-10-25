use std::io::Write;

use super::{IndentLevel, NumberingId};
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberingProperty {
    pub id: Option<NumberingId>,
    pub level: Option<IndentLevel>,
}

impl NumberingProperty {
    pub fn new() -> NumberingProperty {
        Default::default()
    }

    pub fn add_num(mut self, id: NumberingId, level: IndentLevel) -> NumberingProperty {
        self.id = Some(id);
        self.level = Some(level);
        self
    }
}

impl BuildXML for NumberingProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_numbering_property()?
            .add_optional_child(&self.level)?
            .add_optional_child(&self.id)?
            .close()?
            .into_inner()
    }
}
