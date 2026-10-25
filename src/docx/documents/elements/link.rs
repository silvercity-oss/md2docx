use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    val: String,
}

impl Link {
    pub fn new(val: impl Into<String>) -> Link {
        Link {
            val: escape_owned(val.into()),
        }
    }
}

impl BuildXML for Link {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).link(&self.val)?.into_inner()
    }
}
