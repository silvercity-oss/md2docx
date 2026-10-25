use crate::docx::documents::*;
use crate::docx::types::*;
use crate::docx::xml_builder::*;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub level: usize,
    pub start: Start,
    pub format: NumberFormat,
    pub text: LevelText,
    pub jc: LevelJc,
    pub paragraph_property: ParagraphProperty,
    pub run_property: RunProperty,
    pub suffix: LevelSuffixType,
    pub pstyle: Option<ParagraphStyle>,
    pub level_restart: Option<LevelRestart>,
    pub is_lgl: Option<IsLgl>,
}

impl Level {
    pub fn new(level: usize, start: Start, format: NumberFormat, text: LevelText, jc: LevelJc) -> Level {
        Self {
            level,
            start,
            format,
            text,
            jc,
            paragraph_property: ParagraphProperty::new(),
            run_property: RunProperty::new(),
            suffix: LevelSuffixType::Tab,
            pstyle: None,
            level_restart: None,
            is_lgl: None,
        }
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

    // run property
}

impl BuildXML for Level {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_level(&format!("{}", self.level))?
            .add_child(&self.start)?
            .add_child(&self.format)?
            .add_child(&self.text)?
            .add_child(&self.jc)?
            .add_child(&self.paragraph_property)?
            .add_child(&self.run_property)?
            .add_optional_child(&self.pstyle)?
            .add_optional_child(&self.level_restart)?
            .add_optional_child(&self.is_lgl)?
            .apply_if(self.suffix != LevelSuffixType::Tab, |b| {
                b.suffix(&self.suffix.to_string())
            })?
            .close()?
            .into_inner()
    }
}
