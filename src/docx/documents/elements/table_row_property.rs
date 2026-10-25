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
                b.table_row_height(&format!("{h}"), &self.height_rule.unwrap_or_default().to_string())
            })?
            .close()?
            .into_inner()
    }
}
