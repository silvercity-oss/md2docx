use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WpsTextBox {
    pub children: Vec<TextBoxContent>,
    pub has_numbering: bool,
}

impl WpsTextBox {
    pub fn new() -> WpsTextBox {
        Default::default()
    }

    pub fn add_content(mut self, c: TextBoxContent) -> Self {
        if c.has_numbering {
            self.has_numbering = true
        }
        self.children.push(c);
        self
    }
}

impl BuildXML for WpsTextBox {
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

