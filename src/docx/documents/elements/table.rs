use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub rows: Vec<TableChild>,
    pub grid: Vec<usize>,
    pub has_numbering: bool,
    pub property: TableProperty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableChild {
    TableRow(TableRow),
}

impl BuildXML for TableChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            TableChild::TableRow(v) => v.build_to(stream),
        }
    }
}

impl Table {
    pub fn new(rows: Vec<TableRow>) -> Table {
        let property = TableProperty::new();
        let has_numbering = rows.iter().any(|c| c.has_numbering);
        let grid = vec![];
        let rows = rows.into_iter().map(TableChild::TableRow).collect();
        Self {
            property,
            rows,
            grid,
            has_numbering,
        }
    }

    pub fn set_grid(mut self, grid: Vec<usize>) -> Table {
        self.grid = grid;
        self
    }

    pub fn width(mut self, w: usize, t: WidthType) -> Table {
        self.property = self.property.width(w, t);
        self
    }
}

impl BuildXML for Table {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let grid = TableGrid::new(self.grid.clone());
        XMLBuilder::from(stream)
            .open_table()?
            .add_child(&self.property)?
            .add_child(&grid)?
            .add_children(&self.rows)?
            .close()?
            .into_inner()
    }
}
