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

    pub fn add_table_of_contents(mut self, t: TableOfContents) -> Self {
        self.children
            .push(TableCellContent::TableOfContents(Box::new(t)));
        self
    }

    pub fn add_structured_data_tag(mut self, t: StructuredDataTag) -> Self {
        self.children
            .push(TableCellContent::StructuredDataTag(Box::new(t)));
        self
    }

    pub fn add_table(mut self, t: Table) -> TableCell {
        if t.has_numbering {
            self.has_numbering = true
        }
        self.children.push(TableCellContent::Table(Box::new(t)));
        self
    }

    pub fn vertical_merge(mut self, t: VMergeType) -> TableCell {
        self.property = self.property.vertical_merge(t);
        self
    }

    pub fn shading(mut self, s: Shading) -> TableCell {
        self.property = self.property.shading(s);
        self
    }

    pub fn vertical_align(mut self, t: VAlignType) -> TableCell {
        self.property = self.property.vertical_align(t);
        self
    }

    pub fn text_direction(mut self, t: TextDirectionType) -> TableCell {
        self.property = self.property.text_direction(t);
        self
    }

    pub fn grid_span(mut self, v: usize) -> TableCell {
        self.property = self.property.grid_span(v);
        self
    }

    pub fn width(mut self, v: usize, t: WidthType) -> TableCell {
        self.property = self.property.width(v, t);
        self
    }

    pub fn set_border(mut self, border: TableCellBorder) -> Self {
        self.property = self.property.set_border(border);
        self
    }

    pub fn set_borders(mut self, borders: TableCellBorders) -> Self {
        self.property = self.property.set_borders(borders);
        self
    }

    pub fn clear_border(mut self, position: TableCellBorderPosition) -> Self {
        self.property = self.property.clear_border(position);
        self
    }

    pub fn clear_all_border(mut self) -> Self {
        self.property = self.property.clear_all_border();
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

