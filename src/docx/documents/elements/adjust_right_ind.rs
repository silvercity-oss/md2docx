use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct AdjustRightInd(pub isize);

impl AdjustRightInd {
    pub fn new(val: isize) -> AdjustRightInd {
        AdjustRightInd(val)
    }
}

impl BuildXML for AdjustRightInd {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .adjust_right_ind(self.0)?
            .into_inner()
    }
}

