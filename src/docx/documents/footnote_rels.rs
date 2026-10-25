use crate::docx::documents::BuildXML;
use crate::docx::{xml_builder::*, ImageIdAndPath};
use std::io::Write;

/// Relationships of `word/footnotes.xml`: pictures and external links used inside footnotes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FootnoteRels {
    pub images: Vec<ImageIdAndPath>,
    /// (relationship id, target) of external hyperlinks.
    pub hyperlinks: Vec<(String, String)>,
}

impl FootnoteRels {
    pub fn new() -> FootnoteRels {
        Default::default()
    }
}

impl BuildXML for FootnoteRels {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(None)?
            .open_relationships("http://schemas.openxmlformats.org/package/2006/relationships")?
            .apply_each(&self.images, |(id, path), b| {
                b.relationship(
                    id,
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                    path,
                )
            })?
            .apply_each(&self.hyperlinks, |(id, path), b| {
                b.relationship_with_mode(
                    id,
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
                    path,
                    "External",
                )
            })?
            .close()?
            .into_inner()
    }
}
