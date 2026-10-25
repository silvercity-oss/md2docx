use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableCellProperty {
    width: Option<TableCellWidth>,
    borders: Option<TableCellBorders>,
    grid_span: Option<GridSpan>,
    vertical_merge: Option<VMerge>,
    vertical_align: Option<VAlign>,
    text_direction: Option<TextDirection>,
    shading: Option<Shading>,
    margins: Option<CellMargins>,
}

impl TableCellProperty {
    pub fn new() -> TableCellProperty {
        Default::default()
    }

    pub fn shading(mut self, s: Shading) -> Self {
        self.shading = Some(s);
        self
    }
}

impl BuildXML for TableCellProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_cell_property()?
            .add_optional_child(&self.width)?
            .add_optional_child(&self.borders)?
            .add_optional_child(&self.grid_span)?
            .add_optional_child(&self.vertical_merge)?
            .add_optional_child(&self.vertical_align)?
            .add_optional_child(&self.text_direction)?
            .add_optional_child(&self.shading)?
            .add_optional_child(&self.margins)?
            .close()?
            .into_inner()
    }
}
