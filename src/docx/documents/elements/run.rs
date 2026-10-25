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
        Run { ..Default::default() }
    }

    pub fn add_text(mut self, text: impl Into<String>) -> Run {
        self.children
            .push(RunChild::Text(Text::new(text.into().replace('\n', ""))));
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

    pub fn add_image(mut self, pic: Pic) -> Run {
        self.children.push(RunChild::Drawing(Box::new(Drawing::new().pic(pic))));
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

    pub fn style(mut self, style_id: &str) -> Self {
        self.run_property = self.run_property.style(style_id);
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> Run {
        self.run_property = self.run_property.color(color);
        self
    }

    pub fn bold(mut self) -> Run {
        self.run_property = self.run_property.bold();
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

    pub fn underline(mut self, line_type: impl Into<String>) -> Run {
        self.run_property = self.run_property.underline(line_type);
        self
    }

    pub fn add_footnote_reference(mut self, footnote: Footnote) -> Run {
        self.run_property = RunProperty::new().style("FootnoteReference");
        self.children.push(RunChild::FootnoteReference(footnote.into()));
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
