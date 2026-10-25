use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

use crate::docx::line_spacing_type::LineSpacingType;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineSpacing {
    line_rule: Option<LineSpacingType>,
    before: Option<u32>,
    after: Option<u32>,
    before_lines: Option<u32>,
    after_lines: Option<u32>,
    line: Option<i32>,
}

impl LineSpacing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn line_rule(mut self, t: LineSpacingType) -> Self {
        self.line_rule = Some(t);
        self
    }

    pub fn before(mut self, before: u32) -> Self {
        self.before = Some(before);
        self
    }

    pub fn after(mut self, after: u32) -> Self {
        self.after = Some(after);
        self
    }

    pub fn before_lines(mut self, before: u32) -> Self {
        self.before_lines = Some(before);
        self
    }

    pub fn after_lines(mut self, after: u32) -> Self {
        self.after_lines = Some(after);
        self
    }

    pub fn line(mut self, line: i32) -> Self {
        self.line = Some(line);
        self
    }
}

impl BuildXML for LineSpacing {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .line_spacing(
                self.before,
                self.after,
                self.line,
                self.before_lines,
                self.after_lines,
                self.line_rule,
            )?
            .into_inner()
    }
}

