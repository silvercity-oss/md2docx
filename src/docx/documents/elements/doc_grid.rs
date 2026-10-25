use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct DocGrid {
    grid_type: DocGridType,
    line_pitch: Option<usize>,
    char_space: Option<isize>,
}

impl DocGrid {}

impl Default for DocGrid {
    fn default() -> Self {
        Self {
            grid_type: DocGridType::Lines,
            line_pitch: Some(360),
            char_space: None,
        }
    }
}

impl BuildXML for DocGrid {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .doc_grid(&self.grid_type, self.line_pitch, self.char_space)?
            .into_inner()
    }
}
