use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

/// <https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tablepositionproperties?view=openxml-3.0.1>
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TablePositionProperty {
    pub left_from_text: Option<i32>,
    pub right_from_text: Option<i32>,
    pub vertical_anchor: Option<String>,
    pub horizontal_anchor: Option<String>,
    pub position_x_alignment: Option<String>,
    pub position_y_alignment: Option<String>,
    pub position_x: Option<i32>,
    pub position_y: Option<i32>,
}

impl TablePositionProperty {
    pub fn new() -> TablePositionProperty {
        Default::default()
    }

    pub fn left_from_text(mut self, v: i32) -> Self {
        self.left_from_text = Some(v);
        self
    }

    pub fn right_from_text(mut self, v: i32) -> Self {
        self.right_from_text = Some(v);
        self
    }

    pub fn vertical_anchor(mut self, v: impl Into<String>) -> Self {
        self.vertical_anchor = Some(v.into());
        self
    }

    pub fn horizontal_anchor(mut self, v: impl Into<String>) -> Self {
        self.horizontal_anchor = Some(v.into());
        self
    }

    pub fn position_x_alignment(mut self, v: impl Into<String>) -> Self {
        self.position_x_alignment = Some(v.into());
        self
    }

    pub fn position_y_alignment(mut self, v: impl Into<String>) -> Self {
        self.position_y_alignment = Some(v.into());
        self
    }

    pub fn position_x(mut self, v: i32) -> Self {
        self.position_x = Some(v);
        self
    }

    pub fn position_y(mut self, v: i32) -> Self {
        self.position_y = Some(v);
        self
    }
}

impl BuildXML for TablePositionProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .table_position_property(self)?
            .into_inner()
    }
}

