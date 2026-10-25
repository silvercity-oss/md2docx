use std::io::Write;
use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

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

    pub fn without_borders() -> TableProperty {
        TableProperty {
            borders: TableBorders::with_empty(),
            ..Default::default()
        }
    }

    pub fn indent(mut self, v: i32) -> TableProperty {
        self.indent = Some(TableIndent::new(v, WidthType::Dxa));
        self
    }

    pub fn width(mut self, v: usize, t: WidthType) -> TableProperty {
        self.width = TableWidth::new(v, t);
        self
    }

    pub fn align(mut self, v: TableAlignmentType) -> TableProperty {
        self.justification = Justification::new(v.to_string());
        self
    }

    pub fn set_margins(mut self, margins: TableCellMargins) -> Self {
        self.margins = Some(margins);
        self
    }

    pub fn cell_margin_top(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_top(v, t));
        } else {
            let margins = TableCellMargins::new();
            self.margins = Some(margins.margin_top(v, t));
        }
        self
    }

    pub fn cell_margin_right(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_right(v, t));
        } else {
            let margins = TableCellMargins::new();
            self.margins = Some(margins.margin_right(v, t));
        }
        self
    }

    pub fn cell_margin_bottom(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_bottom(v, t));
        } else {
            let margins = TableCellMargins::new();
            self.margins = Some(margins.margin_bottom(v, t));
        }
        self
    }

    pub fn cell_margin_left(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_left(v, t));
        } else {
            let margins = TableCellMargins::new();
            self.margins = Some(margins.margin_left(v, t));
        }
        self
    }

    pub fn set_borders(mut self, borders: TableBorders) -> Self {
        self.borders = borders;
        self
    }

    pub fn set_border(mut self, border: TableBorder) -> Self {
        self.borders = self.borders.set(border);
        self
    }

    pub fn clear_border(mut self, position: TableBorderPosition) -> Self {
        self.borders = self.borders.clear(position);
        self
    }

    pub fn clear_all_border(mut self) -> Self {
        self.borders = self.borders.clear_all();
        self
    }

    pub fn style(mut self, s: impl Into<String>) -> Self {
        self.style = Some(TableStyle::new(s));
        self
    }

    pub fn layout(mut self, t: TableLayoutType) -> Self {
        self.layout = Some(TableLayout::new(t));
        self
    }

    pub fn position(mut self, p: TablePositionProperty) -> Self {
        self.position = Some(p);
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

