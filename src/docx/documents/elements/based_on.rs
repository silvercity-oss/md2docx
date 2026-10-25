use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BasedOn {
    val: String,
}

impl BasedOn {
    pub fn new(val: impl Into<String>) -> BasedOn {
        BasedOn {
            val: escape_owned(val.into()),
        }
    }
}

impl BuildXML for BasedOn {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).based_on(&self.val)?.into_inner()
    }
}

