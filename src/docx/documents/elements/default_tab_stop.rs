use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;
use std::io::Write;


#[derive(Debug, Clone, PartialEq)]
pub struct DefaultTabStop {
    val: usize,
}

impl DefaultTabStop {
    pub fn new(val: usize) -> DefaultTabStop {
        DefaultTabStop { val }
    }
}

impl BuildXML for DefaultTabStop {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .default_tab_stop(self.val)?
            .into_inner()
    }
}

