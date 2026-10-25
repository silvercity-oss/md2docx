use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppProps {}

impl AppProps {
    pub fn new() -> AppProps {
        Default::default()
    }
}

impl BuildXML for AppProps {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_properties(
                "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
                "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes",
            )?
            .close()?
            .into_inner()
    }
}
