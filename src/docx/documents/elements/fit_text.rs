use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct FitText {
    pub val: usize,
    pub id: Option<u32>,
}

impl FitText {
    pub fn new(val: usize) -> Self {
        Self { val, id: None }
    }

    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }
}

impl BuildXML for FitText {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .fit_text(self.val, self.id)?
            .into_inner()
    }
}

