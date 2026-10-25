use std::io::Write;

use crate::docx::documents::*;
use crate::docx::xml_builder::XMLBuilder;

// https://c-rex.net/projects/samples/ooxml/e1/Part4/OOXML_P4_DOCX_PAGEREFPAGEREF_topic_ID0EHXK1.html
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InstrPAGEREF {
    pub page_ref: String,
    pub hyperlink: bool,
    pub relative_position: bool,
}

impl InstrPAGEREF {
    pub fn new(r: impl Into<String>) -> Self {
        Self {
            page_ref: r.into(),
            ..Default::default()
        }
    }

    pub fn hyperlink(mut self) -> Self {
        self.hyperlink = true;
        self
    }

    pub fn relative_position(mut self) -> Self {
        self.relative_position = true;
        self
    }
}

impl BuildXML for InstrPAGEREF {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .plain_text("PAGEREF ")?
            .plain_text(&self.page_ref)?
            .apply_if(self.relative_position, |b| b.plain_text(" \\p"))?
            .apply_if(self.hyperlink, |b| b.plain_text(" \\h"))?
            .into_inner()
    }
}

impl std::str::FromStr for InstrPAGEREF {
    type Err = ();

    fn from_str(instr: &str) -> Result<Self, Self::Err> {
        let mut s = instr.split(' ');
        let text = s.next();
        let mut page_ref = InstrPAGEREF::new(text.unwrap_or_default());
        loop {
            if let Some(i) = s.next() {
                match i {
                    "\\h" => page_ref = page_ref.hyperlink(),
                    "\\p" => page_ref = page_ref.relative_position(),
                    _ => {}
                }
            } else {
                return Ok(page_ref);
            }
        }
    }
}
