use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::{AlignmentType, SpecialIndentType};
use crate::docx::ParagraphBorderPosition;
use crate::docx::{xml_builder::*, TextAlignmentType};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParagraphProperty {
    pub run_property: RunProperty,
    pub style: Option<ParagraphStyle>,
    pub numbering_property: Option<NumberingProperty>,
    pub alignment: Option<Justification>,
    pub indent: Option<Indent>,
    pub line_spacing: Option<LineSpacing>,
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub bidi: Option<bool>,
    pub page_break_before: Option<bool>,
    pub widow_control: Option<bool>,
    pub outline_lvl: Option<OutlineLvl>,
    pub section_property: Option<SectionProperty>,
    pub tabs: Vec<Tab>,
    pub paragraph_property_change: Option<ParagraphPropertyChange>,
    pub borders: Option<ParagraphBorders>,
    pub frame_property: Option<FrameProperty>,
    pub text_alignment: Option<TextAlignment>,
    pub adjust_right_ind: Option<AdjustRightInd>,
    pub snap_to_grid: Option<bool>,
    pub contextual_spacing: Option<bool>,
    pub shading: Option<Shading>,
    // read only
    pub(crate) div_id: Option<String>,
}

// 17.3.1.26
// pPr (Paragraph Properties)
// This element specifies a set of paragraph properties which shall be applied to the contents of the parent
// paragraph after all style/numbering/table properties have been applied to the text. These properties are defined
// as direct formatting, since they are directly applied to the paragraph and supersede any formatting from styles.
impl ParagraphProperty {
    pub fn new() -> ParagraphProperty {
        Default::default()
    }

    pub fn align(mut self, alignment_type: AlignmentType) -> Self {
        self.alignment = Some(Justification::new(alignment_type.to_string()));
        self
    }

    pub fn style(mut self, style_id: &str) -> Self {
        self.style = Some(ParagraphStyle::new(Some(style_id)));
        self
    }

    pub fn indent(
        mut self,
        left: Option<i32>,
        special_indent: Option<SpecialIndentType>,
        end: Option<i32>,
        start_chars: Option<i32>,
    ) -> Self {
        self.indent = Some(Indent::new(left, special_indent, end, start_chars));
        self
    }

    pub fn numbering(mut self, id: NumberingId, level: IndentLevel) -> Self {
        self.numbering_property = Some(NumberingProperty::new().add_num(id, level));
        self
    }

    pub fn numbering_property(mut self, np: NumberingProperty) -> Self {
        self.numbering_property = Some(np);
        self
    }

    pub fn line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.line_spacing = Some(spacing);
        self
    }

    pub fn character_spacing(mut self, spacing: i32) -> Self {
        self.run_property.character_spacing = Some(CharacterSpacing::new(spacing));
        self
    }

    pub fn snap_to_grid(mut self, v: bool) -> Self {
        self.snap_to_grid = Some(v);
        self
    }

    pub fn contextual_spacing(mut self, v: bool) -> Self {
        self.contextual_spacing = Some(v);
        self
    }

    pub fn shading(mut self, s: Shading) -> Self {
        self.shading = Some(s);
        self
    }

    pub fn keep_next(mut self, v: bool) -> Self {
        self.keep_next = Some(v);
        self
    }

    pub fn keep_lines(mut self, v: bool) -> Self {
        self.keep_lines = Some(v);
        self
    }

    pub fn outline_lvl(mut self, v: usize) -> Self {
        if v >= 10 {
            // clamped
            self.outline_lvl = Some(OutlineLvl::new(9));
            return self;
        }
        self.outline_lvl = Some(OutlineLvl::new(v));
        self
    }

    pub fn page_break_before(mut self, v: bool) -> Self {
        self.page_break_before = Some(v);
        self
    }
    pub fn bidi(mut self, v: bool) -> Self {
        self.bidi = Some(v);
        self
    }
    pub fn widow_control(mut self, v: bool) -> Self {
        self.widow_control = Some(v);
        self
    }

    pub fn add_tab(mut self, t: Tab) -> Self {
        self.tabs.push(t);
        self
    }

    pub fn section_property(mut self, s: SectionProperty) -> Self {
        self.section_property = Some(s);
        self
    }

    pub fn paragraph_property_change(mut self, p: ParagraphPropertyChange) -> Self {
        self.paragraph_property_change = Some(p);
        self
    }

    pub fn frame_property(mut self, s: FrameProperty) -> Self {
        self.frame_property = Some(s);
        self
    }

    pub fn run_property(mut self, s: RunProperty) -> Self {
        self.run_property = s;
        self
    }

    pub fn text_alignment(mut self, s: TextAlignmentType) -> Self {
        self.text_alignment = Some(TextAlignment::new(s));
        self
    }

    pub fn adjust_right_ind(mut self, s: isize) -> Self {
        self.adjust_right_ind = Some(AdjustRightInd::new(s));
        self
    }

    // Character-unit indents (hundredths of a character). These keep the
    // absolute values already set by `indent`, so call `indent` first.
    // If no indent exists yet, one is created with only the chars value set.
    pub fn start_chars(mut self, chars: i32) -> Self {
        self.indent = Some(self.indent.take().unwrap_or_default().start_chars(chars));
        self
    }

    pub fn end_chars(mut self, chars: i32) -> Self {
        self.indent = Some(self.indent.take().unwrap_or_default().end_chars(chars));
        self
    }

    pub fn hanging_chars(mut self, chars: i32) -> Self {
        self.indent = Some(self.indent.take().unwrap_or_default().hanging_chars(chars));
        self
    }

    pub fn first_line_chars(mut self, chars: i32) -> Self {
        self.indent = Some(
            self.indent
                .take()
                .unwrap_or_default()
                .first_line_chars(chars),
        );
        self
    }

    pub fn set_borders(mut self, borders: ParagraphBorders) -> Self {
        self.borders = Some(borders);
        self
    }

    pub fn set_border(mut self, border: ParagraphBorder) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().set(border));
        self
    }

    pub fn clear_border(mut self, position: ParagraphBorderPosition) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().clear(position));
        self
    }

    pub fn clear_all_borders(mut self) -> Self {
        self.borders = Some(self.borders.unwrap_or_default().clear_all());
        self
    }
}

impl BuildXML for ParagraphProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_paragraph_property()?
            .add_child(&self.run_property)?
            .add_optional_child(&self.style)?
            .add_optional_child(&self.numbering_property)?
            .add_optional_child(&self.frame_property)?
            .add_optional_child(&self.alignment)?
            .add_optional_child(&self.indent)?
            .add_optional_child(&self.line_spacing)?
            .add_optional_child(&self.outline_lvl)?
            .add_optional_child(&self.paragraph_property_change)?
            .add_optional_child(&self.borders)?
            .add_optional_child(&self.shading)?
            .add_optional_child(&self.text_alignment)?
            .add_optional_child(&self.adjust_right_ind)?
            .apply_opt(self.snap_to_grid, |v, b| b.snap_to_grid(v))?
            .apply_opt(self.contextual_spacing, |v, b| b.contextual_spacing(v))?
            .apply_if(self.keep_next, |b| b.keep_next())?
            .apply_if(self.keep_lines, |b| b.keep_lines())?
            .apply_if(self.page_break_before, |b| b.page_break_before())?
            .apply_if(self.bidi, |b| b.bidi())?
            .apply_opt(self.widow_control, |flag, b| {
                b.widow_control(if flag { "1" } else { "0" })
            })?
            .apply_if(!self.tabs.is_empty(), |b| {
                b.open_tabs()?
                    .apply_each(&self.tabs, |tab, b| b.tab(tab.val, tab.leader, tab.pos))?
                    .close()
            })?
            .add_optional_child(&self.section_property)?
            .close()?
            .into_inner()
    }
}

