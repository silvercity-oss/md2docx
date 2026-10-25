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
