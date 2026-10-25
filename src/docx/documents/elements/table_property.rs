use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct TableProperty {
    width: TableWidth,
    justification: Justification,
    borders: TableBorders,
    margins: Option<TableCellMargins>,
    indent: Option<TableIndent>,
    style: Option<TableStyle>,
    layout: Option<TableLayout>,
    position: Option<TablePositionProperty>,
}

impl Default for TableProperty {
    fn default() -> Self {
        TableProperty {
            width: TableWidth::new(0, WidthType::Auto),
            justification: Justification::new("left"),
            borders: TableBorders::new(),
            margins: None,
            indent: None,
            style: None,
            layout: None,
            position: None,
        }
    }
}

impl TableProperty {
    pub fn new() -> TableProperty {
        Default::default()
    }

    pub fn width(mut self, v: usize, t: WidthType) -> TableProperty {
        self.width = TableWidth::new(v, t);
        self
    }
}

impl BuildXML for TableProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_property()?
            // If a style is set, it has to come first, otherwise everything
            // before the style might be ignored by word.
            .add_optional_child(&self.style)?
            .add_child(&self.width)?
            .add_child(&self.justification)?
            .add_child(&self.borders)?
            .add_optional_child(&self.margins)?
            .add_optional_child(&self.indent)?
            .add_optional_child(&self.layout)?
            .add_optional_child(&self.position)?
            .close()?
            .into_inner()
    }
}
