use super::{XMLBuilder, XmlEvent};
use crate::docx::xml::writer::Result;
use std::io::Write;

impl<W: Write> XMLBuilder<W> {
    /// Build XML declaration
    /// i.e. `<?xml version="1.0" encoding="UTF-8"?>`
    pub(crate) fn declaration(self, standalone: Option<bool>) -> Result<Self> {
        self.write(XmlEvent::StartDocument {
            version: super::XmlVersion::Version10,
            encoding: Some("UTF-8"),
            standalone,
        })
    }
}

