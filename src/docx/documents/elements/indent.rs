use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Indent {
    pub start: Option<i32>,
    pub end: Option<i32>,
    pub special_indent: Option<SpecialIndentType>,
    // Character-unit indents in hundredths of a character (e.g. 400 = 4 chars).
    // When set, Word uses these in preference to the absolute values above.
    pub start_chars: Option<i32>,
    pub end_chars: Option<i32>,
    pub hanging_chars: Option<i32>,
    pub first_line_chars: Option<i32>,
}

impl Indent {
    pub fn new(
        start: Option<i32>,
        special_indent: Option<SpecialIndentType>,
        end: Option<i32>,
        start_chars: Option<i32>,
    ) -> Indent {
        Indent {
            start,
            start_chars,
            end,
            special_indent,
            end_chars: None,
            hanging_chars: None,
            first_line_chars: None,
        }
    }

    pub fn end(mut self, end: i32) -> Self {
        self.end = Some(end);
        self
    }

    pub fn start_chars(mut self, chars: i32) -> Self {
        self.start_chars = Some(chars);
        self
    }

    pub fn end_chars(mut self, chars: i32) -> Self {
        self.end_chars = Some(chars);
        self
    }

    pub fn hanging_chars(mut self, chars: i32) -> Self {
        self.hanging_chars = Some(chars);
        self
    }

    pub fn first_line_chars(mut self, chars: i32) -> Self {
        self.first_line_chars = Some(chars);
        self
    }
}

impl Default for Indent {
    fn default() -> Self {
        Indent::new(None, None, None, None)
    }
}

impl BuildXML for Indent {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .indent(
                self.start,
                self.special_indent,
                self.end.unwrap_or_default(),
                self.start_chars,
                self.end_chars,
                self.hanging_chars,
                self.first_line_chars,
            )?
            .into_inner()
    }
}

