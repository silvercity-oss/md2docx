use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct PositionalTab {
    pub alignment: PositionalTabAlignmentType,
    pub relative_to: PositionalTabRelativeTo,
    pub leader: TabLeaderType,
}

impl Default for PositionalTab {
    fn default() -> Self {
        Self {
            alignment: PositionalTabAlignmentType::Left,
            relative_to: PositionalTabRelativeTo::Margin,
            leader: TabLeaderType::None,
        }
    }
}

impl PositionalTab {}

impl BuildXML for PositionalTab {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .ptab(self.alignment, self.relative_to, self.leader)?
            .into_inner()
    }
}
