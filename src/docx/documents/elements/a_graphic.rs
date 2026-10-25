use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AGraphic {
    pub children: Vec<AGraphicData>,
}

impl AGraphic {
    pub fn new() -> AGraphic {
        Default::default()
    }

    pub fn add_graphic_data(mut self, g: AGraphicData) -> Self {
        self.children.push(g);
        self
    }
}

impl BuildXML for AGraphic {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_graphic("http://schemas.openxmlformats.org/drawingml/2006/main")?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}

