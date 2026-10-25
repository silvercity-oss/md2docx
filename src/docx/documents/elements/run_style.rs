use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct RunStyle {
    pub val: String,
}

impl Default for RunStyle {
    fn default() -> Self {
        RunStyle {
            val: "Normal".to_owned(),
        }
    }
}

impl RunStyle {
    pub fn new(val: impl Into<String>) -> RunStyle {
        RunStyle {
            val: escape_owned(val.into()),
        }
    }
}

impl BuildXML for RunStyle {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).run_style(&self.val)?.into_inner()
    }
}

