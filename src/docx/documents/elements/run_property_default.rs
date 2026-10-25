use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct RunPropertyDefault {
    run_property: RunProperty,
}

impl RunPropertyDefault {
    pub fn new() -> RunPropertyDefault {
        Default::default()
    }

    pub fn size(mut self, size: usize) -> Self {
        self.run_property = self.run_property.size(size);
        self
    }

    pub fn fonts(mut self, font: RunFonts) -> Self {
        self.run_property = self.run_property.fonts(font);
        self
    }
}

impl Default for RunPropertyDefault {
    fn default() -> Self {
        let run_property = RunProperty::new();
        RunPropertyDefault { run_property }
    }
}

impl BuildXML for RunPropertyDefault {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_run_property_default()?
            .add_child(&self.run_property)?
            .close()?
            .into_inner()
    }
}
