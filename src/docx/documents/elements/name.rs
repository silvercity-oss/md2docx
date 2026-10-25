use std::io::Write;

use std::str::FromStr;

use crate::docx::documents::BuildXML;
use crate::docx::{escape, xml_builder::*};

#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    name: String,
}

impl Name {
    pub fn new(name: impl Into<String>) -> Name {
        Name {
            name: escape::escape_owned(name.into()),
        }
    }

    pub fn is_heading(&self) -> bool {
        self.name.to_lowercase().starts_with("heading")
    }

    pub fn get_heading_number(&self) -> Option<usize> {
        let replaced = self.name.to_lowercase().replace("heading ", "");
        usize::from_str(&replaced).ok()
    }
}

impl BuildXML for Name {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).name(&self.name)?.into_inner()
    }
}
