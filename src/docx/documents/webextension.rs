use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape;
use crate::docx::xml_builder::*;

#[derive(Debug, PartialEq, Clone)]
pub struct WebExtensionProperty {
    pub name: String,
    pub value: String,
}

impl WebExtensionProperty {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct WebExtension {
    pub id: String,
    pub version: String,
    pub store: String,
    pub store_type: String,
    pub reference_id: String,
    pub properties: Vec<WebExtensionProperty>,
}

impl WebExtension {
    pub fn new(
        id: impl Into<String>,
        reference_id: impl Into<String>,
        version: impl Into<String>,
        store: impl Into<String>,
        store_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            reference_id: reference_id.into(),
            version: version.into(),
            store: store.into(),
            store_type: store_type.into(),
            properties: vec![],
        }
    }

    pub fn property(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let v = value.into();
        let v = format!("&quot;{}&quot;", escape(&v).replace("&quot;", "\\&quot;"));
        self.properties.push(WebExtensionProperty::new(name, v));
        self
    }
}

impl BuildXML for WebExtension {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_webextension(
                "http://schemas.microsoft.com/office/webextensions/webextension/2010/11",
                &format!("{{{}}}", self.id),
            )?
            .webextension_reference(
                &self.reference_id,
                &self.version,
                &self.store,
                &self.store_type,
            )?
            .webextension_alternate_references()?
            .open_webextension_properties()?
            .apply_each(&self.properties, |p, b| {
                b.webextension_property(&p.name, &p.value)
            })?
            .close()?
            .webextension_bindings()?
            .webextension_snapshot(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            )?
            .close()?
            .into_inner()
    }
}

