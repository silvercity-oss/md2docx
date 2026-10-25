use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WpsShape {
    children: Vec<WpsShapeChild>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WpsShapeChild {
    WpsTextBox(WpsTextBox),
}

impl WpsShape {
    pub fn new() -> WpsShape {
        Default::default()
    }

    pub fn add_text_box(mut self, text_box: WpsTextBox) -> Self {
        self.children.push(WpsShapeChild::WpsTextBox(text_box));
        self
    }
}

impl BuildXML for WpsShapeChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            WpsShapeChild::WpsTextBox(t) => t.build_to(stream),
        }
    }
}

impl BuildXML for WpsShape {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_wp_text_box()?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}
