use std::io::Write;

use super::*;
use crate::docx::xml_builder::*;
use crate::docx::{documents::BuildXML, HeightRule};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableRowProperty {
    grid_after: Option<u32>,
    width_after: Option<f32>,
    grid_before: Option<u32>,
    width_before: Option<f32>,
    row_height: Option<f32>,
    height_rule: Option<HeightRule>,
    pub del: Option<Delete>,
    pub ins: Option<Insert>,
    pub cant_split: Option<CantSplit>,
}

impl TableRowProperty {
    pub fn new() -> TableRowProperty {
        Default::default()
    }

    pub fn grid_after(mut self, after: u32) -> Self {
        self.grid_after = Some(after);
        self
    }

    pub fn width_after(mut self, w: f32) -> Self {
        self.width_after = Some(w);
        self
    }

    pub fn grid_before(mut self, before: u32) -> Self {
        self.grid_before = Some(before);
        self
    }

    pub fn width_before(mut self, w: f32) -> Self {
        self.width_before = Some(w);
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = Some(h);
        self
    }

    pub fn height_rule(mut self, r: HeightRule) -> Self {
        self.height_rule = Some(r);
        self
    }

    pub fn delete(mut self, d: Delete) -> Self {
        self.del = Some(d);
        self
    }

    pub fn insert(mut self, i: Insert) -> Self {
        self.ins = Some(i);
        self
    }

    pub fn cant_split(mut self) -> Self {
        self.cant_split = Some(CantSplit::default());
        self
    }
}

impl BuildXML for TableRowProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_table_row_property()?
            .add_optional_child(&self.del)?
            .add_optional_child(&self.ins)?
            .add_optional_child(&self.cant_split)?
            .apply_opt(self.row_height, |h, b| {
                b.table_row_height(
                    &format!("{h}"),
                    &self.height_rule.unwrap_or_default().to_string(),
                )
            })?
            .close()?
            .into_inner()
    }
}

