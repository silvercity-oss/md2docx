use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteText {
    text: String,
    preserve_space: bool,
}

impl DeleteText {
    pub fn new(text: impl Into<String>) -> DeleteText {
        DeleteText {
            text: escape_owned(text.into()),
            preserve_space: true,
        }
    }

    pub(crate) fn without_escape(text: impl Into<String>) -> DeleteText {
        DeleteText {
            text: text.into(),
            preserve_space: true,
        }
    }
}

impl BuildXML for DeleteText {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .delete_text(&self.text, true)?
            .into_inner()
    }
}

