use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CellMargin {
    pub val: usize,
    pub width_type: WidthType,
}

impl CellMargin {
    pub fn new(val: usize, t: WidthType) -> Self {
        Self { val, width_type: t }
    }
}

impl Default for CellMargin {
    fn default() -> CellMargin {
        CellMargin {
            val: 55,
            width_type: WidthType::Dxa,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellMargins {
    top: Option<CellMargin>,
    left: Option<CellMargin>,
    bottom: Option<CellMargin>,
    right: Option<CellMargin>,
}

impl CellMargins {
    pub fn new() -> CellMargins {
        Default::default()
    }

    pub fn margin_top(mut self, v: usize, t: WidthType) -> Self {
        self.top = Some(CellMargin::new(v, t));
        self
    }

    pub fn margin_right(mut self, v: usize, t: WidthType) -> Self {
        self.right = Some(CellMargin::new(v, t));
        self
    }

    pub fn margin_left(mut self, v: usize, t: WidthType) -> Self {
        self.left = Some(CellMargin::new(v, t));
        self
    }

    pub fn margin_bottom(mut self, v: usize, t: WidthType) -> Self {
        self.bottom = Some(CellMargin::new(v, t));
        self
    }
}

impl BuildXML for CellMargins {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_cell_margins()?
            .apply_opt(self.top.as_ref(), |top, b| {
                b.margin_top(top.val as i32, top.width_type)
            })?
            .apply_opt(self.left.as_ref(), |left, b| {
                b.margin_left(left.val as i32, left.width_type)
            })?
            .apply_opt(self.bottom.as_ref(), |bottom, b| {
                b.margin_bottom(bottom.val as i32, bottom.width_type)
            })?
            .apply_opt(self.right.as_ref(), |right, b| {
                b.margin_right(right.val as i32, right.width_type)
            })?
            .close()?
            .into_inner()
    }
}

