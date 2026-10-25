use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use crate::docx::Tab;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tabs {
    pub tabs: Vec<Tab>,
}

impl Tabs {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add_tab(mut self, t: Tab) -> Self {
        self.tabs.push(t);
        self
    }
}

impl BuildXML for Tabs {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .apply_each(&self.tabs, |t, b| b.tab(t.val, t.leader, t.pos))?
            .into_inner()
    }
}
