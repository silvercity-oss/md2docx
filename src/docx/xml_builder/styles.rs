use super::XMLBuilder;
use super::XmlEvent;
use crate::docx::xml::writer::Result;
use std::io::Write;

impl<W: Write> XMLBuilder<W> {
    // Build w:style element
    // i.e. <w:styles ... >
    pub(crate) fn open_styles(self) -> Result<Self> {
        self.write(
            XmlEvent::start_element("w:styles")
                .attr(
                    "xmlns:mc",
                    "http://schemas.openxmlformats.org/markup-compatibility/2006",
                )
                .attr(
                    "xmlns:r",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                )
                .attr(
                    "xmlns:w",
                    "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                )
                .attr("xmlns:w14", "http://schemas.microsoft.com/office/word/2010/wordml")
                .attr("xmlns:w15", "http://schemas.microsoft.com/office/word/2012/wordml")
                .attr("mc:Ignorable", "w14 w15"),
        )
    }
}
