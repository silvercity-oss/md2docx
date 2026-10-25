use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;

//17.7.4.14
// qFormat (Primary Style)
// This element specifies whether this style shall be treated as a primary style when this document is loaded by an
// application. If this element is set, then this style has been designated as being particularly important for the
// current document, and this information can be used by an application in any means desired. [Note: This setting
// 637ECMA-376 Part 1 does not imply any behavior for the style, only that the style is of particular significance for this document. end note]
#[derive(Default)]
pub struct QFormat {}

impl QFormat {
    pub fn new() -> QFormat {
        Default::default()
    }
}

impl BuildXML for QFormat {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream).q_format()?.into_inner()
    }
}

