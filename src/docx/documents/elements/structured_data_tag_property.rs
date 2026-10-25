use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
// use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredDataTagProperty {
    pub run_property: RunProperty,
    pub data_binding: Option<DataBinding>,
    pub alias: Option<String>,
}

impl Default for StructuredDataTagProperty {
    fn default() -> Self {
        Self {
            run_property: RunProperty::new(),
            data_binding: None,
            alias: None,
        }
    }
}

impl StructuredDataTagProperty {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn alias(mut self, v: impl Into<String>) -> Self {
        self.alias = Some(v.into());
        self
    }
}

impl BuildXML for StructuredDataTagProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_structured_tag_property()?
            .add_child(&self.run_property)?
            .add_optional_child(&self.data_binding)?
            .apply_opt(self.alias.as_ref(), |alias, b| b.alias(alias))?
            .close()?
            .into_inner()
    }
}
