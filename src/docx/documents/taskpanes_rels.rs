use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, PartialEq, Clone, Default)]
pub struct TaskpanesRels {
    pub rels: Vec<(String, String, String)>,
}

impl TaskpanesRels {
    pub fn new() -> Self {
        Default::default()
    }
}

impl BuildXML for TaskpanesRels {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_relationships("http://schemas.openxmlformats.org/package/2006/relationships")?
            .apply_each(&self.rels, |(k, id, v), b| b.relationship(id, k, v))?
            .close()?
            .into_inner()
    }
}
