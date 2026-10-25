use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone)]
pub struct TableGrid {
    grid: Vec<usize>,
}

impl TableGrid {
    pub fn new(grid: Vec<usize>) -> TableGrid {
        TableGrid { grid }
    }
}

impl BuildXML for TableGrid {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_grid()?
            .apply_each(&self.grid, |g, b| b.grid_column(*g as i32, WidthType::Dxa))?
            .close()?
            .into_inner()
    }
}
