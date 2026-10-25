use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tab {
    pub val: Option<TabValueType>,
    pub leader: Option<TabLeaderType>,
    pub pos: Option<usize>,
}

impl Tab {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn val(mut self, v: TabValueType) -> Self {
        self.val = Some(v);
        self
    }

    pub fn leader(mut self, v: TabLeaderType) -> Self {
        self.leader = Some(v);
        self
    }

    pub fn pos(mut self, v: usize) -> Self {
        self.pos = Some(v);
        self
    }
}

impl BuildXML for Tab {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .tab(self.val, self.leader, self.pos)?
            .into_inner()
    }
}
