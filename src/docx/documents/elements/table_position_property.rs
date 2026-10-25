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

impl TablePositionProperty {}

impl BuildXML for TablePositionProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).table_position_property(self)?.into_inner()
    }
}
