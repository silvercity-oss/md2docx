use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::escape::escape_owned;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use crate::docx::StyleType;

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub style_id: String,
    pub is_default: bool,
    pub name: Name,
    pub style_type: StyleType,
    pub run_property: RunProperty,
    pub paragraph_property: ParagraphProperty,
    pub table_property: TableProperty,
    pub table_cell_property: TableCellProperty,
    pub based_on: Option<BasedOn>,
    pub next: Option<Next>,
    pub link: Option<Link>,
    pub q_format: bool,
    pub ui_priority: Option<usize>,
    pub semi_hidden: bool,
    pub unhide_when_used: bool,
}

impl Default for Style {
    fn default() -> Self {
        let name = Name::new("");
        let rpr = RunProperty::new();
        let ppr = ParagraphProperty::new();
        Style {
            style_id: "".to_owned(),
            is_default: false,
            style_type: StyleType::Paragraph,
            name,
            run_property: rpr,
            paragraph_property: ppr,
            table_property: TableProperty::new(),
            table_cell_property: TableCellProperty::new(),
            based_on: None,
            next: None,
            link: None,
            q_format: true,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
        }
    }
}

impl Style {
    pub fn new(style_id: impl Into<String>, style_type: StyleType) -> Self {
        let default = Default::default();
        Style {
            style_id: escape_owned(style_id.into()),
            style_type,
            ..default
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Name::new(name);
        self
    }

    pub fn size(mut self, size: usize) -> Self {
        self.run_property = self.run_property.size(size);
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.run_property = self.run_property.color(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.run_property = self.run_property.bold();
        self
    }

    pub fn underline(mut self, line_type: impl Into<String>) -> Self {
        self.run_property = self.run_property.underline(line_type);
        self
    }

    pub fn fonts(mut self, f: RunFonts) -> Self {
        self.run_property = self.run_property.fonts(f);
        self
    }

    pub fn align(mut self, alignment_type: AlignmentType) -> Self {
        self.paragraph_property = self.paragraph_property.align(alignment_type);
        self
    }

    pub fn snap_to_grid(mut self, v: bool) -> Self {
        self.paragraph_property = self.paragraph_property.snap_to_grid(v);
        self
    }

    pub fn line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.paragraph_property = self.paragraph_property.line_spacing(spacing);
        self
    }

    pub fn indent(
        mut self,
        left: Option<i32>,
        special_indent: Option<SpecialIndentType>,
        end: Option<i32>,
        start_chars: Option<i32>,
    ) -> Self {
        self.paragraph_property = self.paragraph_property.indent(left, special_indent, end, start_chars);
        self
    }

    pub fn outline_lvl(mut self, l: usize) -> Self {
        self.paragraph_property = self.paragraph_property.outline_lvl(l);
        self
    }

    // frameProperty
}

impl BuildXML for Style {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        // Set "Normal" as default if you need change these values please fix it
        XMLBuilder::from(stream)
            .open_style(self.style_type, &self.style_id, self.is_default)?
            .add_child(&self.name)?
            .add_child(&self.run_property)?
            .add_child(&self.paragraph_property)?
            .apply_if(self.style_type == StyleType::Table, |b| {
                b.add_child(&self.table_cell_property)?.add_child(&self.table_property)
            })?
            .add_optional_child(&self.next)?
            .add_optional_child(&self.link)?
            .apply_if(self.q_format, |b| b.add_child(&QFormat::new()))?
            .apply_if(self.ui_priority.is_some(), |b| {
                b.ui_priority(self.ui_priority.unwrap_or_default())
            })?
            .apply_if(self.semi_hidden, |b| b.semi_hidden())?
            .apply_if(self.unhide_when_used, |b| b.unhide_when_used())?
            .add_optional_child(&self.based_on)?
            .close()?
            .into_inner()
    }
}
