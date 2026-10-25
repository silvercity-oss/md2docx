use super::*;
use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub run_property: RunProperty,
    pub children: Vec<RunChild>,
}

impl Default for Run {
    fn default() -> Self {
        let run_property = RunProperty::new();
        Self {
            run_property,
            children: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunChild {
    Text(Text),
    Sym(Sym),
    DeleteText(DeleteText),
    Tab(Tab),
    PTab(PositionalTab),
    Break(Break),
    CarriageReturn(CarriageReturn),
    NoBreakHyphen(NoBreakHyphen),
    SoftHyphen(SoftHyphen),
    Drawing(Box<Drawing>),
    Shape(Box<Shape>),
    CommentStart(Box<CommentRangeStart>),
    CommentEnd(CommentRangeEnd),
    FieldChar(FieldChar),
    InstrText(Box<InstrText>),
    DeleteInstrText(Box<DeleteInstrText>),
    // For reader
    InstrTextString(String),
    FootnoteReference(FootnoteReference),
    Shading(Shading),
}

impl Run {
    pub fn new() -> Run {
        Run {
            ..Default::default()
        }
    }

    pub fn add_text(mut self, text: impl Into<String>) -> Run {
        self.children
            .push(RunChild::Text(Text::new(text.into().replace('\n', ""))));
        self
    }

    pub(crate) fn add_text_without_escape(mut self, text: impl Into<String>) -> Run {
        self.children.push(RunChild::Text(Text::without_escape(
            text.into().replace('\n', ""),
        )));
        self
    }

    pub fn add_delete_text(mut self, text: impl Into<String>) -> Run {
        self.children.push(RunChild::DeleteText(DeleteText::new(
            text.into().replace('\n', ""),
        )));
        self
    }

    pub(crate) fn add_delete_text_without_escape(mut self, text: impl Into<String>) -> Run {
        self.children
            .push(RunChild::DeleteText(DeleteText::without_escape(
                text.into().replace('\n', ""),
            )));
        self
    }

    pub fn add_field_char(mut self, t: crate::docx::types::FieldCharType, dirty: bool) -> Run {
        let mut f = FieldChar::new(t);
        if dirty {
            f = f.dirty();
        };
        self.children.push(RunChild::FieldChar(f));
        self
    }

    pub fn add_tc(mut self, tc: InstrTC) -> Run {
        self = self.add_field_char(crate::docx::types::FieldCharType::Begin, false);
        self = self.add_instr_text(InstrText::TC(tc));
        self = self.add_field_char(crate::docx::types::FieldCharType::End, false);
        self
    }

    pub fn add_instr_text(mut self, i: InstrText) -> Run {
        self.children.push(RunChild::InstrText(Box::new(i)));
        self
    }

    pub fn add_delete_instr_text(mut self, i: DeleteInstrText) -> Run {
        self.children.push(RunChild::DeleteInstrText(Box::new(i)));
        self
    }

    pub fn add_tab(mut self) -> Run {
        self.children.push(RunChild::Tab(Tab::new()));
        self
    }

    pub fn add_ptab(mut self, ptab: PositionalTab) -> Run {
        self.children.push(RunChild::PTab(ptab));
        self
    }

    pub fn add_image(mut self, pic: Pic) -> Run {
        self.children
            .push(RunChild::Drawing(Box::new(Drawing::new().pic(pic))));
        self
    }

    pub(crate) fn add_drawing(mut self, d: Drawing) -> Run {
        self.children.push(RunChild::Drawing(Box::new(d)));
        self
    }

    // For now reader only
    //    pub(crate) fn add_shape(mut self, d: Shape) -> Run {
    //        self.children.push(RunChild::Shape(Box::new(d)));
    //        self
    //    }

    pub fn add_break(mut self, break_type: BreakType) -> Run {
        self.children.push(RunChild::Break(Break::new(break_type)));
        self
    }

    pub fn add_carriage_return(mut self) -> Run {
        self.children
            .push(RunChild::CarriageReturn(CarriageReturn::new()));
        self
    }

    pub fn add_no_break_hyphen(mut self) -> Run {
        self.children
            .push(RunChild::NoBreakHyphen(NoBreakHyphen::new()));
        self
    }

    pub fn add_soft_hyphen(mut self) -> Run {
        self.children.push(RunChild::SoftHyphen(SoftHyphen::new()));
        self
    }

    pub fn add_sym(mut self, sym: Sym) -> Run {
        self.children.push(RunChild::Sym(sym));
        self
    }

    pub fn style(mut self, style_id: &str) -> Self {
        self.run_property = self.run_property.style(style_id);
        self
    }

    pub fn size(mut self, size: usize) -> Run {
        self.run_property = self.run_property.size(size);
        self
    }

    pub fn character_spacing(mut self, v: i32) -> Run {
        self.run_property = self.run_property.spacing(v);
        self
    }

    pub fn stretch(mut self, v: i32) -> Run {
        self.run_property = self.run_property.stretch(v);
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> Run {
        self.run_property = self.run_property.color(color);
        self
    }

    /// Sets the theme color reference (`w:themeColor`) on the run color.
    pub fn theme_color(mut self, theme_color: crate::docx::types::ThemeColor) -> Run {
        self.run_property = self.run_property.theme_color(theme_color);
        self
    }

    /// Sets the theme shade modifier (`w:themeShade`) on the run color.
    pub fn theme_shade(mut self, theme_shade: impl Into<String>) -> Run {
        self.run_property = self.run_property.theme_shade(theme_shade);
        self
    }

    /// Sets the theme tint modifier (`w:themeTint`) on the run color.
    pub fn theme_tint(mut self, theme_tint: impl Into<String>) -> Run {
        self.run_property = self.run_property.theme_tint(theme_tint);
        self
    }

    pub fn highlight(mut self, color: impl Into<String>) -> Run {
        self.run_property = self.run_property.highlight(color);
        self
    }

    pub fn bold(mut self) -> Run {
        self.run_property = self.run_property.bold();
        self
    }

    pub fn disable_bold(mut self) -> Run {
        self.run_property = self.run_property.disable_bold();
        self
    }

    pub fn italic(mut self) -> Run {
        self.run_property = self.run_property.italic();
        self
    }

    pub fn strike(mut self) -> Run {
        self.run_property = self.run_property.strike();
        self
    }

    pub fn dstrike(mut self) -> Run {
        self.run_property = self.run_property.dstrike();
        self
    }

    pub fn text_border(mut self, b: TextBorder) -> Run {
        self.run_property = self.run_property.text_border(b);
        self
    }

    pub fn disable_italic(mut self) -> Run {
        self.run_property = self.run_property.disable_italic();
        self
    }

    pub fn underline(mut self, line_type: impl Into<String>) -> Run {
        self.run_property = self.run_property.underline(line_type);
        self
    }

    pub fn vanish(mut self) -> Run {
        self.run_property = self.run_property.vanish();
        self
    }

    pub fn fonts(mut self, f: RunFonts) -> Run {
        self.run_property = self.run_property.fonts(f);
        self
    }

    pub(crate) fn set_property(mut self, p: RunProperty) -> Run {
        self.run_property = p;
        self
    }

    pub fn add_footnote_reference(mut self, footnote: Footnote) -> Run {
        self.run_property = RunProperty::new().style("FootnoteReference");
        self.children
            .push(RunChild::FootnoteReference(footnote.into()));
        self
    }

    pub fn shading(mut self, shading: Shading) -> Run {
        self.run_property = self.run_property.shading(shading);
        self
    }
}

impl BuildXML for RunChild {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        match self {
            RunChild::Text(t) => t.build_to(stream),
            RunChild::Sym(t) => t.build_to(stream),
            RunChild::DeleteText(t) => t.build_to(stream),
            RunChild::Tab(t) => t.build_to(stream),
            RunChild::PTab(t) => t.build_to(stream),
            RunChild::Break(t) => t.build_to(stream),
            RunChild::CarriageReturn(t) => t.build_to(stream),
            RunChild::NoBreakHyphen(t) => t.build_to(stream),
            RunChild::SoftHyphen(t) => t.build_to(stream),
            RunChild::Drawing(t) => t.build_to(stream),
            RunChild::Shape(_t) => {
                todo!("Support shape writer.")
            }
            RunChild::CommentStart(c) => c.build_to(stream),
            RunChild::CommentEnd(c) => c.build_to(stream),
            RunChild::FieldChar(c) => c.build_to(stream),
            RunChild::InstrText(c) => c.build_to(stream),
            RunChild::DeleteInstrText(c) => c.build_to(stream),
            RunChild::InstrTextString(instruction) => XMLBuilder::from(stream)
                .write(XmlEvent::start_element("w:instrText").attr("xml:space", "preserve"))?
                .plain_text(&crate::docx::escape::escape(instruction))?
                .close()?
                .into_inner(),
            RunChild::FootnoteReference(c) => c.build_to(stream),
            RunChild::Shading(s) => s.build_to(stream),
        }
    }
}

impl BuildXML for Run {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_run()?
            .add_child(&self.run_property)?
            .add_children(&self.children)?
            .close()?
            .into_inner()
    }
}

