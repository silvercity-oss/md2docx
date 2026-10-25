use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

// 22.1.2.51
// jc (Justification)
// This element specifies justification of the math paragraph (a series of adjacent instances of mathematical text
// within the same paragraph). A math paragraph can be Left Justified, Right Justified, Centered, or Centered as
// Group. If this element is omitted, the math paragraph is Centered as Group. Whether the element is absent or
// present without the val attribute, the default of the val attribute is centerGroup . This means that the instances
// of mathematical text can be aligned with respect to each other, but the entire group of mathematical text is
// centered as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct Justification {
    pub val: String,
}

impl Justification {
    pub fn new(val: impl Into<String>) -> Justification {
        Justification { val: val.into() }
    }
}

impl BuildXML for Justification {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .justification(&self.val)?
            .into_inner()
    }
}

