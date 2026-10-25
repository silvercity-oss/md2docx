use std::collections::BTreeMap;
use std::io::{Read, Write};

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, PartialEq, Clone)]
pub struct ContentTypes {
    types: BTreeMap<String, String>,
    web_extension_count: usize,
    custom_xml_count: usize,
    header_count: usize,
    footer_count: usize,
}

impl ContentTypes {
    pub fn new() -> ContentTypes {
        Default::default()
    }

    pub fn set_default(mut self) -> ContentTypes {
        self.types.insert(
            "/_rels/.rels".to_owned(),
            "application/vnd.openxmlformats-package.relationships+xml".to_owned(),
        );
        self.types.insert(
            "/docProps/app.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.extended-properties+xml".to_owned(),
        );
        self.types.insert(
            "/docProps/core.xml".to_owned(),
            "application/vnd.openxmlformats-package.core-properties+xml".to_owned(),
        );
        self.types.insert(
            "/word/_rels/document.xml.rels".to_owned(),
            "application/vnd.openxmlformats-package.relationships+xml".to_owned(),
        );
        self.types.insert(
            "/word/settings.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml".to_owned(),
        );
        self.types.insert(
            "/word/fontTable.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml".to_owned(),
        );
        self.types.insert(
            "/word/document.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml".to_owned(),
        );
        self.types.insert(
            "/word/styles.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml".to_owned(),
        );
        self.types.insert(
            "/word/comments.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml".to_owned(),
        );
        self.types.insert(
            "/word/numbering.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml".to_owned(),
        );
        self.types.insert(
            "/word/commentsExtended.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml".to_owned(),
        );
        self.types.insert(
            "/docProps/custom.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.custom-properties+xml".to_owned(),
        );
        self
    }

    pub fn add_footer(mut self) -> Self {
        self.footer_count += 1;
        self.types.insert(
            format!("/word/footer{}.xml", self.footer_count),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml".to_owned(),
        );
        self
    }
    pub fn add_footnotes(mut self) -> Self {
        self.types.insert(
            "/word/footnotes.xml".to_owned(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml".to_owned(),
        );
        self
    }
}

impl Default for ContentTypes {
    fn default() -> Self {
        ContentTypes {
            types: BTreeMap::new(),
            web_extension_count: 1,
            custom_xml_count: 1,
            header_count: 0,
            footer_count: 0,
        }
    }
}

impl BuildXML for ContentTypes {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(None)?
            .open_types("http://schemas.openxmlformats.org/package/2006/content-types")?
            .add_default("png", "image/png")?
            .add_default("jpeg", "image/jpeg")?
            .add_default("jpg", "image/jpg")?
            .add_default("bmp", "image/bmp")?
            .add_default("gif", "image/gif")?
            .add_default("rels", "application/vnd.openxmlformats-package.relationships+xml")?
            .add_default("xml", "application/xml")?
            .apply_each(self.types.iter(), |(k, v), b| b.add_override(k, v))?
            .close()?
            .into_inner()
    }
}
