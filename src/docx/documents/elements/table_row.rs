use std::io::Write;

use super::{Delete, Insert, TableCell, TableRowProperty};
use crate::docx::xml_builder::*;
use crate::docx::{documents::BuildXML, HeightRule};

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableRowChild>,
    pub has_numbering: bool,
    pub property: TableRowProperty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableRowChild {
    TableCell(TableCell),
}

impl BuildXML for TableRowChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            TableRowChild::TableCell(v) => v.build_to(stream),
        }
    }
}

impl TableRow {
    pub fn new(cells: Vec<TableCell>) -> TableRow {
        let property = TableRowProperty::new();
        let has_numbering = cells.iter().any(|c| c.has_numbering);
        let cells = cells.into_iter().map(TableRowChild::TableCell).collect();
        Self {
            cells,
            property,
            has_numbering,
        }
    }

    pub fn grid_after(mut self, grid_after: u32) -> TableRow {
        self.property = self.property.grid_after(grid_after);
        self
    }

    pub fn width_after(mut self, w: f32) -> TableRow {
        self.property = self.property.width_after(w);
        self
    }

    pub fn grid_before(mut self, grid_before: u32) -> TableRow {
        self.property = self.property.grid_before(grid_before);
        self
    }

    pub fn width_before(mut self, w: f32) -> TableRow {
        self.property = self.property.width_before(w);
        self
    }

    pub fn row_height(mut self, h: f32) -> TableRow {
        self.property = self.property.row_height(h);
        self
    }

    pub fn height_rule(mut self, r: HeightRule) -> TableRow {
        self.property = self.property.height_rule(r);
        self
    }

    pub fn delete(mut self, d: Delete) -> TableRow {
        self.property = self.property.delete(d);
        self
    }

    pub fn insert(mut self, i: Insert) -> TableRow {
        self.property = self.property.insert(i);
        self
    }

    pub fn cant_split(mut self) -> TableRow {
        self.property = self.property.cant_split();
        self
    }
}

impl BuildXML for TableRow {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_row()?
            .add_child(&self.property)?
            .add_children(&self.cells)?
            .close()?
            .into_inner()
    }
}

