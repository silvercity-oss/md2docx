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

    pub fn spacing(mut self, spacing: i32) -> RunProperty {
        self.character_spacing = Some(CharacterSpacing::new(spacing));
        self
    }

    pub fn color(mut self, color: impl Into<String>) -> RunProperty {
        self.color = Some(Color::new(color));
        self
    }

    /// Sets the theme color reference (`w:themeColor`) on the run color.
    ///
    /// The hex value set via [`color`](Self::color) is kept as a fallback. If no
    /// color has been set yet this defaults the fallback to `auto`.
    pub fn theme_color(mut self, theme_color: crate::docx::types::ThemeColor) -> RunProperty {
        let color = self.color.take().unwrap_or_else(|| Color::new("auto"));
        self.color = Some(color.theme_color(theme_color));
        self
    }

    /// Sets the theme shade modifier (`w:themeShade`) on the run color.
    pub fn theme_shade(mut self, theme_shade: impl Into<String>) -> RunProperty {
        let color = self.color.take().unwrap_or_else(|| Color::new("auto"));
        self.color = Some(color.theme_shade(theme_shade));
        self
    }

    /// Sets the theme tint modifier (`w:themeTint`) on the run color.
    pub fn theme_tint(mut self, theme_tint: impl Into<String>) -> RunProperty {
        let color = self.color.take().unwrap_or_else(|| Color::new("auto"));
        self.color = Some(color.theme_tint(theme_tint));
        self
    }

    pub fn highlight(mut self, color: impl Into<String>) -> RunProperty {
        self.highlight = Some(Highlight::new(color));
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

    pub fn disable_bold(mut self) -> RunProperty {
        self.bold = Some(Bold::new().disable());
        self.bold_cs = Some(BoldCs::new().disable());
        self
    }

    pub fn caps(mut self) -> RunProperty {
        self.caps = Some(Caps::new());
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

    pub fn disable_strike(mut self) -> RunProperty {
        self.strike = Some(Strike::new().disable());
        self
    }

    pub fn dstrike(mut self) -> RunProperty {
        self.dstrike = Some(Dstrike::new());
        self.strike = None;
        self
    }

    pub fn disable_dstrike(mut self) -> RunProperty {
        self.dstrike = Some(Dstrike::new().disable());
        self
    }

    pub fn disable_italic(mut self) -> RunProperty {
        self.italic = Some(Italic::new().disable());
        self.italic_cs = Some(ItalicCs::new().disable());
        self
    }

    pub fn underline(mut self, line_type: impl Into<String>) -> RunProperty {
        self.underline = Some(Underline::new(line_type));
        self
    }

    pub fn vanish(mut self) -> RunProperty {
        self.vanish = Some(Vanish::new());
        self
    }

    pub fn spec_vanish(mut self) -> RunProperty {
        self.spec_vanish = Some(SpecVanish::new());
        self
    }

    pub fn fonts(mut self, font: RunFonts) -> RunProperty {
        self.fonts = Some(font);
        self
    }

    pub fn character_spacing(mut self, v: i32) -> RunProperty {
        self.character_spacing = Some(CharacterSpacing::new(v));
        self
    }

    pub fn stretch(mut self, v: i32) -> RunProperty {
        self.stretch = Some(Stretch::new(v));
        self
    }

    pub fn fit_text(mut self, val: usize, id: Option<u32>) -> RunProperty {
        let mut fit_text = FitText::new(val);
        if let Some(id) = id {
            fit_text = fit_text.id(id);
        }
        self.fit_text = Some(fit_text);
        self
    }

    pub fn text_border(mut self, b: TextBorder) -> Self {
        self.text_border = Some(b);
        self
    }

    pub fn delete(mut self, d: Delete) -> Self {
        self.del = Some(d);
        self
    }

    pub fn insert(mut self, i: Insert) -> Self {
        self.ins = Some(i);
        self
    }

    pub fn ptab(mut self, ptab: PositionalTab) -> Self {
        self.positional_tab = Some(ptab);
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
            .add_optional_child(&self.sz)?
            .add_optional_child(&self.sz_cs)?
            .add_optional_child(&self.color)?
            .add_optional_child(&self.bold)?
            .add_optional_child(&self.bold_cs)?
            .add_optional_child(&self.caps)?
            .add_optional_child(&self.italic)?
            .add_optional_child(&self.italic_cs)?
            .add_optional_child(&self.strike)?
            .add_optional_child(&self.dstrike)?
            .add_optional_child(&self.highlight)?
            .add_optional_child(&self.underline)?
            .add_optional_child(&self.vanish)?
            .add_optional_child(&self.spec_vanish)?
            .add_optional_child(&self.fonts)?
            .add_optional_child(&self.text_border)?
            .add_optional_child(&self.ins)?
            .add_optional_child(&self.del)?
            .add_optional_child(&self.vert_align)?
            .add_optional_child(&self.character_spacing)?
            .add_optional_child(&self.fit_text)?
            .add_optional_child(&self.stretch)?
            .add_optional_child(&self.style)?
            .add_optional_child(&self.positional_tab)?
            .add_optional_child(&self.shading)?
            .close()?
            .into_inner()
    }
}

