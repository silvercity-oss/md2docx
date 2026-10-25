use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Styles {
    pub doc_defaults: DocDefaults,
    pub styles: Vec<Style>,
}

impl Styles {
    pub fn new() -> Styles {
        Default::default()
    }

    pub fn add_style(mut self, style: Style) -> Self {
        self.styles.push(style);
        self
    }

    pub fn default_size(mut self, size: usize) -> Self {
        self.doc_defaults = self.doc_defaults.size(size);
        self
    }

    pub fn default_line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.doc_defaults = self.doc_defaults.line_spacing(spacing);
        self
    }

    pub fn default_fonts(mut self, font: RunFonts) -> Self {
        self.doc_defaults = self.doc_defaults.fonts(font);
        self
    }

    pub fn create_heading_style_map(&self) -> std::collections::HashMap<String, usize> {
        self.styles
            .iter()
            .filter_map(|s| {
                if s.name.is_heading() {
                    let n = s.name.get_heading_number();
                    n.map(|n| (s.style_id.clone(), n))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            doc_defaults: DocDefaults::new(),
            styles: vec![],
        }
    }
}

impl BuildXML for Styles {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_styles()?
            .add_child(&self.doc_defaults)?
            .add_optional_child(
                &self
                    .styles
                    .iter()
                    .find(|style| style.style_id == "Normal")
                    .is_none()
                    .then_some(Style::new("Normal", StyleType::Paragraph).name("Normal")),
            )?
            .add_children(&self.styles)?
            .close()?
            .into_inner()
    }
}
