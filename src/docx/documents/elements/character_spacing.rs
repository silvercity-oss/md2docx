use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct CharacterSpacing {
    value: i32,
}

impl CharacterSpacing {
    pub fn new(s: i32) -> CharacterSpacing {
        Self { value: s }
    }
}

impl BuildXML for CharacterSpacing {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).spacing(self.value)?.into_inner()
    }
}

