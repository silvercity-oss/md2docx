use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

// https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.frameproperties?view=openxml-3.0.1
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameProperty {
    /// Frame Height
    /// Represents the following attribute in the schema: w:h
    pub h: Option<u32>,
    /// Frame Height Type
    /// Represents the following attribute in the schema: w:hRule
    pub h_rule: Option<String>,
    /// Frame Horizontal Positioning Base
    /// Represents the following attribute in the schema: w:hAnchor
    pub h_anchor: Option<String>,
    /// Horizontal Frame Padding
    /// Represents the following attribute in the schema: w:hSpace
    pub h_space: Option<i32>,
    pub v_anchor: Option<String>,
    pub v_space: Option<i32>,
    pub w: Option<u32>,
    /// Text Wrapping Around Frame
    /// Represents the following attribute in the schema: w:wrap
    pub wrap: Option<String>,
    /// Absolute Horizontal Position
    /// Represents the following attribute in the schema: w:x
    pub x: Option<i32>,
    /// Relative Horizontal Position
    /// Represents the following attribute in the schema: w:xAlign
    pub x_align: Option<String>,
    /// Absolute Vertical Position
    /// Represents the following attribute in the schema: w:y
    pub y: Option<i32>,
    /// Relative Vertical Position
    /// Represents the following attribute in the schema: w:yAlign
    pub y_align: Option<String>,
}

impl FrameProperty {}

impl BuildXML for FrameProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).frame_property(self)?.into_inner()
    }
}
