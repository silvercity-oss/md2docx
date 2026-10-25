use crate::docx::documents::BuildXML;
use crate::docx::{xml_builder::*, ImageIdAndPath};
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FooterRels {
    pub images: Vec<(String, String)>,
}

impl FooterRels {
    pub fn new() -> FooterRels {
        Default::default()
    }

    pub fn add_image(mut self, id: impl Into<String>, path: impl Into<String>) -> Self {
        self.images.push((id.into(), path.into()));
        self
    }

    pub(crate) fn set_images(&mut self, images: Vec<ImageIdAndPath>) {
        self.images = images;
    }
}

impl BuildXML for FooterRels {
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
            .close()?
            .into_inner()
    }
}
