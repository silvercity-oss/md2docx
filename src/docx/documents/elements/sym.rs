use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Sym {
    pub font: String,
    pub char: String,
}

impl Sym {
    pub fn new(font: impl Into<String>, char: impl Into<String>) -> Self {
        Self {
            font: font.into(),
            char: char.into(),
        }
    }
}
impl BuildXML for Sym {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .sym(&self.font, &self.char)?
            .into_inner()
    }
}

