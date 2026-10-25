use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Numbering {
    pub id: usize,
    pub abstract_num_id: usize,
    pub level_overrides: Vec<LevelOverride>,
}

impl Numbering {
    pub fn new(id: usize, abstract_num_id: usize) -> Self {
        Self {
            id,
            abstract_num_id,
            level_overrides: vec![],
        }
    }

    pub fn add_override(mut self, o: LevelOverride) -> Self {
        self.level_overrides.push(o);
        self
    }
}

impl BuildXML for Numbering {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        let id = format!("{}", self.id);
        let abs_id = format!("{}", self.abstract_num_id);
        XMLBuilder::from(stream)
            .open_num(&id)?
            .abstract_num_id(&abs_id)?
            .add_children(&self.level_overrides)?
            .close()?
            .into_inner()
    }
}
