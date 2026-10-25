use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreProps {
    config: CorePropsConfig,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CorePropsConfig {
    created: Option<String>,
    creator: Option<String>,
    description: Option<String>,
    language: Option<String>,
    last_modified_by: Option<String>,
    modified: Option<String>,
    revision: Option<usize>,
    subject: Option<String>,
    title: Option<String>,
}

impl CoreProps {
    pub(crate) fn new(config: CorePropsConfig) -> CoreProps {
        CoreProps { config }
    }

    pub fn created_at(mut self, date: &str) -> Self {
        self.config.created = Some(date.to_owned());
        self
    }

    pub fn updated_at(mut self, date: &str) -> Self {
        self.config.modified = Some(date.to_owned());
        self
    }
}

impl CorePropsConfig {
    pub fn new() -> Self {
        CorePropsConfig {
            created: None,
            creator: None,
            description: None,
            language: None,
            last_modified_by: None,
            modified: None,
            revision: None,
            subject: None,
            title: None,
        }
    }
}

impl BuildXML for CoreProps {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_core_properties(
                "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
                "http://purl.org/dc/elements/1.1/",
                "http://purl.org/dc/terms/",
                "http://purl.org/dc/dcmitype/",
                "http://www.w3.org/2001/XMLSchema-instance",
            )?
            .dcterms_created(
                "dcterms:W3CDTF",
                self.config.created.as_deref().unwrap_or("1970-01-01T00:00:00Z"),
            )?
            .dc_creator(self.config.creator.as_deref().unwrap_or("unknown"))?
            .cp_last_modified_by(self.config.last_modified_by.as_deref().unwrap_or("unknown"))?
            .dcterms_modified(
                "dcterms:W3CDTF",
                self.config.modified.as_deref().unwrap_or("1970-01-01T00:00:00Z"),
            )?
            .cp_revision(&self.config.revision.unwrap_or(1).to_string())?
            .apply_opt(self.config.description.as_ref(), |v, b| b.dc_description(v))?
            .apply_opt(self.config.language.as_ref(), |v, b| b.dc_language(v))?
            .apply_opt(self.config.subject.as_ref(), |v, b| b.dc_subject(v))?
            .apply_opt(self.config.title.as_ref(), |v, b| b.dc_title(v))?
            .close()?
            .into_inner()
    }
}
