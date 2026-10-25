use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;


/*
17.9.8 lvlOverride (Numbering Level Definition Override)
This element specifies an optional override which shall be applied in place of zero or more levels from the abstract numbering definition for a given numbering definition instance. Each instance of this element is used to override the appearance and behavior of a given numbering level definition within the given abstract numbering definition.
*/
#[derive(Debug, Clone, PartialEq)]
pub struct LevelOverride {
    pub level: usize,
    pub override_start: Option<usize>,
    pub override_level: Option<Level>,
}

impl LevelOverride {
    pub fn new(level: usize) -> LevelOverride {
        LevelOverride {
            level,
            override_start: None,
            override_level: None,
        }
    }

    pub fn start(mut self, start: usize) -> LevelOverride {
        self.override_start = Some(start);
        self
    }

    pub fn level(mut self, override_level: Level) -> LevelOverride {
        self.override_level = Some(override_level);
        self
    }
}

impl BuildXML for LevelOverride {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_level_override(&format!("{}", self.level))?
            .add_optional_child(&self.override_level)?
            .apply_opt(self.override_start, |start, b| {
                b.start_override(&format!("{start}"))
            })?
            .close()?
            .into_inner()
    }
}

