use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct Taskpanes {}

impl Taskpanes {
    pub fn new() -> Self {
        Default::default()
    }
}

impl BuildXML for Taskpanes {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_taskpanes("http://schemas.microsoft.com/office/webextensions/taskpanes/2010/11")?
            .open_taskpane("", "1", "350", "1")?
            .webextensionref(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                "rId1",
            )?
            .close()?
            .close()?
            .into_inner()
    }
}

