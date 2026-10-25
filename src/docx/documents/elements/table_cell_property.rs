use std::io::Write;
use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

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

    pub fn width(mut self, v: usize, t: WidthType) -> TableCellProperty {
        self.width = Some(TableCellWidth::new(v, t));
        self
    }

    pub fn vertical_merge(mut self, t: VMergeType) -> TableCellProperty {
        self.vertical_merge = Some(VMerge::new(t));
        self
    }

    pub fn vertical_align(mut self, t: VAlignType) -> TableCellProperty {
        self.vertical_align = Some(VAlign::new(t));
        self
    }

    pub fn text_direction(mut self, t: TextDirectionType) -> Self {
        self.text_direction = Some(TextDirection::new(t));
        self
    }

    pub fn grid_span(mut self, v: usize) -> TableCellProperty {
        self.grid_span = Some(GridSpan::new(v));
        self
    }

    pub fn shading(mut self, s: Shading) -> Self {
        self.shading = Some(s);
        self
    }

    pub fn set_borders(mut self, borders: TableCellBorders) -> Self {
        self.borders = Some(borders);
        self
    }

    pub fn set_border(mut self, border: TableCellBorder) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().set(border));
        self
    }

    pub fn clear_border(mut self, position: TableCellBorderPosition) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().clear(position));
        self
    }

    pub fn clear_all_border(mut self) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().clear_all());
        self
    }

    pub fn margins(mut self, margins: CellMargins) -> Self {
        self.margins = Some(margins);
        self
    }

    pub fn margin_top(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_top(v, t));
        } else {
            let margins = CellMargins::new();
            self.margins = Some(margins.margin_top(v, t));
        }
        self
    }

    pub fn margin_right(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_right(v, t));
        } else {
            let margins = CellMargins::new();
            self.margins = Some(margins.margin_right(v, t));
        }
        self
    }

    pub fn margin_bottom(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_bottom(v, t));
        } else {
            let margins = CellMargins::new();
            self.margins = Some(margins.margin_bottom(v, t));
        }
        self
    }

    pub fn margin_left(mut self, v: usize, t: WidthType) -> Self {
        if let Some(margins) = self.margins {
            self.margins = Some(margins.margin_left(v, t));
        } else {
            let margins = CellMargins::new();
            self.margins = Some(margins.margin_left(v, t));
        }
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

