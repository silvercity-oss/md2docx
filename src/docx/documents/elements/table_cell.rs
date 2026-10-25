use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub children: Vec<TableCellContent>,
    pub property: TableCellProperty,
    pub has_numbering: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableCellContent {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
    StructuredDataTag(Box<StructuredDataTag>),
    TableOfContents(Box<TableOfContents>),
}

impl TableCell {
    pub fn new() -> TableCell {
        Default::default()
    }

    pub fn add_paragraph(mut self, p: Paragraph) -> TableCell {
        if p.has_numbering {
            self.has_numbering = true
        }
        self.children.push(TableCellContent::Paragraph(Box::new(p)));
        self
    }

    pub fn add_table(mut self, t: Table) -> TableCell {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(TableCellContent::Table(Box::new(t)));
        self
    }

    pub fn shading(mut self, s: Shading) -> TableCell {
        self.property = self.property.shading(s);
        self
    }
}

impl Default for TableCell {
    fn default() -> Self {
        let property = TableCellProperty::new();
        let children = vec![];
        Self {
            property,
            children,
            has_numbering: false,
        }
    }
}

impl BuildXML for TableCell {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_cell()?
            .add_child(&self.property)?
            .apply_each(&self.children, |ch, b| {
                match ch {
                    TableCellContent::Paragraph(p) => b.add_child(&**p),
                    TableCellContent::Table(t) => {
                        b.add_child(&**t)?
                            // INFO: We need to add empty paragraph when parent cell includes only cell.
                            .apply_if(self.children.len() == 1, |b| b.add_child(&Paragraph::new()))
                    }
                    TableCellContent::StructuredDataTag(t) => b.add_child(&t),
                    TableCellContent::TableOfContents(t) => b.add_child(&t),
                }
            })?
            // INFO: We need to add empty paragraph when parent cell includes only cell.
            .apply_if(self.children.is_empty(), |b| b.add_child(&Paragraph::new()))?
            .close()?
            .into_inner()
    }
}
