use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Rels {
    pub rels: Vec<(String, String, String)>,
}

impl Rels {
    pub fn new() -> Rels {
        Default::default()
    }

    pub fn set_default(mut self) -> Self {
        self.rels.push((
            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".to_owned(),
            "rId1".to_owned(),
            "docProps/core.xml".to_owned(),
        ));
        self.rels.push((
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties".to_owned(),
            "rId2".to_owned(),
            "docProps/app.xml".to_owned(),
        ));
        self.rels.push((
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument".to_owned(),
            "rId3".to_owned(),
            "word/document.xml".to_owned(),
        ));
        self.rels.push((
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties".to_owned(),
            "rId4".to_owned(),
            "docProps/custom.xml".to_owned(),
        ));
        self
    }
}

impl BuildXML for Rels {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(None)?
            .open_relationships("http://schemas.openxmlformats.org/package/2006/relationships")?
            .apply_each(&self.rels, |(k, id, v), b| b.relationship(id, k, v))?
            .close()?
            .into_inner()
    }
}
