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

    pub fn without_borders(rows: Vec<TableRow>) -> Table {
        let property = TableProperty::without_borders();
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

    pub fn add_row(mut self, row: TableRow) -> Table {
        self.rows.push(TableChild::TableRow(row));
        self
    }

    pub fn set_grid(mut self, grid: Vec<usize>) -> Table {
        self.grid = grid;
        self
    }

    pub fn indent(mut self, v: i32) -> Table {
        self.property = self.property.indent(v);
        self
    }

    pub fn align(mut self, v: TableAlignmentType) -> Table {
        self.property = self.property.align(v);
        self
    }

    pub fn style(mut self, s: impl Into<String>) -> Table {
        self.property = self.property.style(s);
        self
    }

    pub fn layout(mut self, t: TableLayoutType) -> Table {
        self.property = self.property.layout(t);
        self
    }

    pub fn position(mut self, p: TablePositionProperty) -> Self {
        self.property = self.property.position(p);
        self
    }

    pub fn width(mut self, w: usize, t: WidthType) -> Table {
        self.property = self.property.width(w, t);
        self
    }

    pub fn margins(mut self, margins: TableCellMargins) -> Self {
        self.property = self.property.set_margins(margins);
        self
    }

    pub fn set_borders(mut self, borders: TableBorders) -> Self {
        self.property = self.property.set_borders(borders);
        self
    }

    pub fn set_border(mut self, border: TableBorder) -> Self {
        self.property = self.property.set_border(border);
        self
    }

    pub fn clear_border(mut self, position: TableBorderPosition) -> Self {
        self.property = self.property.clear_border(position);
        self
    }

    pub fn clear_all_border(mut self) -> Self {
        self.property = self.property.clear_all_border();
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

