use std::io::Write;

use crate::docx::documents::*;
use crate::docx::escape;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphPropertyChange {
    pub author: String,
    pub date: String,
    pub property: Box<ParagraphProperty>,
}

impl Default for ParagraphPropertyChange {
    fn default() -> ParagraphPropertyChange {
        Self {
            author: "unnamed".to_owned(),
            date: "1970-01-01T00:00:00Z".to_owned(),
            property: Default::default(),
        }
    }
}

impl ParagraphPropertyChange {}

impl ParagraphPropertyChangeId for ParagraphPropertyChange {}

impl BuildXML for ParagraphPropertyChange {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = self.generate();
        XMLBuilder::from(stream)
            .open_paragraph_property_change(id.as_ref(), &self.author, &self.date)?
            .add_child(&self.property)?
            .close()?
            .into_inner()
    }
}
