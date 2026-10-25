use super::Font;
use crate::docx::documents::BuildXML;
use crate::docx::types::FontPitchType;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontTable {}

impl FontTable {
    pub fn new() -> FontTable {
        Default::default()
    }
}

impl BuildXML for FontTable {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let b = XMLBuilder::from(stream);
        let times = Font::new("Times New Roman", "00", "roman", FontPitchType::Variable);
        let symbol = Font::new("Symbol", "02", "roman", FontPitchType::Variable);
        let arial = Font::new("Arial", "00", "swiss", FontPitchType::Variable);
        b.declaration(Some(true))?
            .open_fonts()?
            .add_child(&times)?
            .add_child(&symbol)?
            .add_child(&arial)?
            .close()?
            .into_inner()
    }
}
