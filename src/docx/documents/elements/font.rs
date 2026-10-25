use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug)]
pub struct Font<'a> {
    name: &'a str,
    charset: &'a str,
    family: &'a str,
    pitch: FontPitchType,
}

impl<'a> Font<'a> {
    pub fn new(name: &'a str, charset: &'a str, family: &'a str, pitch: FontPitchType) -> Font<'a> {
        Font {
            name,
            charset,
            family,
            pitch,
        }
    }
}

impl<'a> BuildXML for Font<'a> {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_font(self.name)?
            .charset(self.charset)?
            .family(self.family)?
            .pitch(&self.pitch.to_string())?
            .close()?
            .into_inner()
    }
}

