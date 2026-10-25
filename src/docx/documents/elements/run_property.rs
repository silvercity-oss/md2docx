use std::io::Write;

use super::*;
use crate::docx::documents::BuildXML;
use crate::docx::types::*;
use crate::docx::xml_builder::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunProperty {
    pub style: Option<RunStyle>,
    pub sz: Option<Sz>,
    pub sz_cs: Option<SzCs>,
    pub color: Option<Color>,
    pub highlight: Option<Highlight>,
    pub vert_align: Option<VertAlign>,
    pub underline: Option<Underline>,
    pub bold: Option<Bold>,
    pub bold_cs: Option<BoldCs>,
    pub caps: Option<Caps>,
    pub italic: Option<Italic>,
    pub italic_cs: Option<ItalicCs>,
    pub vanish: Option<Vanish>,
    pub spec_vanish: Option<SpecVanish>,
    pub character_spacing: Option<CharacterSpacing>,
    pub fit_text: Option<FitText>,
    pub stretch: Option<Stretch>,
    pub fonts: Option<RunFonts>,
    pub text_border: Option<TextBorder>,
    pub del: Option<Delete>,
    pub ins: Option<Insert>,
    pub strike: Option<Strike>,
    pub dstrike: Option<Dstrike>,
    pub positional_tab: Option<PositionalTab>,
    pub shading: Option<Shading>,
}

impl RunProperty {
    pub fn new() -> RunProperty {
        Default::default()
    }

    pub fn style(mut self, style_id: &str) -> Self {
        self.style = Some(RunStyle::new(style_id));
        self
    }

    pub fn size(mut self, size: usize) -> RunProperty {
        self.sz = Some(Sz::new(size));
        self.sz_cs = Some(SzCs::new(size));
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> RunProperty {
        self.color = Some(Color::new(color));
        self
    }

    pub fn vert_align(mut self, a: VertAlignType) -> Self {
        self.vert_align = Some(VertAlign::new(a));
        self
    }

    pub fn bold(mut self) -> RunProperty {
        self.bold = Some(Bold::new());
        self.bold_cs = Some(BoldCs::new());
        self
    }

    pub fn italic(mut self) -> RunProperty {
        self.italic = Some(Italic::new());
        self.italic_cs = Some(ItalicCs::new());
        self
    }

    pub fn strike(mut self) -> RunProperty {
        self.strike = Some(Strike::new());
        self.dstrike = None;
        self
    }

    pub fn underline(mut self, line_type: impl Into<String>) -> RunProperty {
        self.underline = Some(Underline::new(line_type));
        self
    }

    pub fn fonts(mut self, font: RunFonts) -> RunProperty {
        self.fonts = Some(font);
        self
    }

    pub fn shading(mut self, s: Shading) -> Self {
        self.shading = Some(s);
        self
    }
}

impl BuildXML for RunProperty {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .open_run_property()?
            // Tracked-change marks first (paragraph-mark properties only), then the schema's
            // `CT_RPr` order.
            .add_optional_child(&self.ins)?
            .add_optional_child(&self.del)?
            .add_optional_child(&self.style)?
            .add_optional_child(&self.fonts)?
            .add_optional_child(&self.bold)?
            .add_optional_child(&self.bold_cs)?
            .add_optional_child(&self.italic)?
            .add_optional_child(&self.italic_cs)?
            .add_optional_child(&self.caps)?
            .add_optional_child(&self.strike)?
            .add_optional_child(&self.dstrike)?
            .add_optional_child(&self.vanish)?
            .add_optional_child(&self.color)?
            .add_optional_child(&self.character_spacing)?
            .add_optional_child(&self.stretch)?
            .add_optional_child(&self.sz)?
            .add_optional_child(&self.sz_cs)?
            .add_optional_child(&self.highlight)?
            .add_optional_child(&self.underline)?
            .add_optional_child(&self.text_border)?
            .add_optional_child(&self.shading)?
            .add_optional_child(&self.fit_text)?
            .add_optional_child(&self.vert_align)?
            .add_optional_child(&self.spec_vanish)?
            .add_optional_child(&self.positional_tab)?
            .close()?
            .into_inner()
    }
}
