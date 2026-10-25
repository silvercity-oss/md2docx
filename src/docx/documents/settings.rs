use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::CharacterSpacingValues;
use crate::docx::xml_builder::*;


#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    default_tab_stop: DefaultTabStop,
    zoom: Zoom,
    doc_id: Option<DocId>,
    doc_vars: Vec<DocVar>,
    even_and_odd_headers: bool,
    adjust_line_height_in_table: bool,
    character_spacing_control: Option<CharacterSpacingValues>,
}

impl Settings {
    pub fn new() -> Settings {
        Default::default()
    }

    pub fn doc_id(mut self, id: impl Into<String>) -> Self {
        self.doc_id = Some(DocId::new(id.into()));
        self
    }

    pub fn default_tab_stop(mut self, tab_stop: usize) -> Self {
        self.default_tab_stop = DefaultTabStop::new(tab_stop);
        self
    }

    pub fn add_doc_var(mut self, name: impl Into<String>, val: impl Into<String>) -> Self {
        self.doc_vars.push(DocVar::new(name, val));
        self
    }

    pub fn even_and_odd_headers(mut self) -> Self {
        self.even_and_odd_headers = true;
        self
    }

    pub fn adjust_line_height_in_table(mut self) -> Self {
        self.adjust_line_height_in_table = true;
        self
    }

    pub fn character_spacing_control(mut self, val: CharacterSpacingValues) -> Self {
        self.character_spacing_control = Some(val);
        self
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_tab_stop: DefaultTabStop::new(840),
            zoom: Zoom::new(100),
            doc_id: None,
            doc_vars: vec![],
            even_and_odd_headers: false,
            adjust_line_height_in_table: false,
            character_spacing_control: None,
        }
    }
}

impl BuildXML for Settings {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .declaration(Some(true))?
            .open_settings()?
            .add_child(&self.default_tab_stop)?
            .add_child(&self.zoom)?
            .open_compat()?
            .space_for_ul()?
            .balance_single_byte_double_byte_width()?
            .do_not_leave_backslash_alone()?
            .ul_trail_space()?
            .do_not_expand_shift_return()?
            .apply_opt(self.character_spacing_control, |v, b| {
                b.character_spacing_control(&v.to_string())
            })?
            .apply_if(self.adjust_line_height_in_table, |b| {
                b.adjust_line_height_table()
            })?
            .use_fe_layout()?
            .compat_setting(
                "compatibilityMode",
                "http://schemas.microsoft.com/office/word",
                "15",
            )?
            .compat_setting(
                "overrideTableStyleFontSizeAndJustification",
                "http://schemas.microsoft.com/office/word",
                "1",
            )?
            .compat_setting(
                "enableOpenTypeFeatures",
                "http://schemas.microsoft.com/office/word",
                "1",
            )?
            .compat_setting(
                "doNotFlipMirrorIndents",
                "http://schemas.microsoft.com/office/word",
                "1",
            )?
            .compat_setting(
                "differentiateMultirowTableHeaders",
                "http://schemas.microsoft.com/office/word",
                "1",
            )?
            .compat_setting(
                "useWord2013TrackBottomHyphenation",
                "http://schemas.microsoft.com/office/word",
                "0",
            )?
            .close()?
            .add_optional_child(&self.doc_id)?
            .apply_if(!self.doc_vars.is_empty(), |b| {
                b.open_doc_vars()?.add_children(&self.doc_vars)?.close()
            })?
            .apply_if(self.even_and_odd_headers, |b| b.even_and_odd_headers())?
            .close()?
            .into_inner()
    }
}

