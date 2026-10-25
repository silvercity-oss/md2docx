use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    pub preserve_space: bool,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Text {
        Text {
            text: escape_owned(text.into()),
            preserve_space: true,
        }
    }

    pub(crate) fn without_escape(text: impl Into<String>) -> Text {
        Text {
            text: text.into(),
            preserve_space: true,
        }
    }
}

impl BuildXML for Text {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .text(&self.text, true)?
            .into_inner()
    }
}

