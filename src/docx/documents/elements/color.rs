use std::io::Write;

use crate::docx::documents::BuildXML;
use crate::docx::types::ThemeColor;
use crate::docx::xml_builder::*;

/// A `<w:color>` color property.
///
/// `val` is an `RRGGBB` hex string (or `auto`). The optional theme fields let
/// the color reference a document-theme color instead of being locked to the
/// hex value: when present they emit `w:themeColor` / `w:themeShade` /
/// `w:themeTint`, and Word resolves the color from the active theme while
/// `val` stays as a fallback for renderers that ignore themes.
///
#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    val: String,
    theme_color: Option<String>,
    theme_shade: Option<String>,
    theme_tint: Option<String>,
}

impl Color {
    /// Creates a color from an `RRGGBB` hex string (or `auto`).
    pub fn new(val: impl Into<String>) -> Color {
        Color {
            val: val.into(),
            theme_color: None,
            theme_shade: None,
            theme_tint: None,
        }
    }

    /// Sets the theme color reference (`w:themeColor`).
    pub fn theme_color(mut self, theme_color: ThemeColor) -> Color {
        // Stored as the serialized ST_ThemeColor token (e.g. "accent1") so the
        // builder and reader share a single string representation; the typed
        // `ThemeColor` enum is the ergonomic boundary for callers.
        self.theme_color = Some(theme_color.to_string());
        self
    }

    /// Sets the theme shade modifier (`w:themeShade`), a hex byte such as `"BF"`.
    pub fn theme_shade(mut self, theme_shade: impl Into<String>) -> Color {
        self.theme_shade = Some(theme_shade.into());
        self
    }

    /// Sets the theme tint modifier (`w:themeTint`), a hex byte such as `"99"`.
    pub fn theme_tint(mut self, theme_tint: impl Into<String>) -> Color {
        self.theme_tint = Some(theme_tint.into());
        self
    }
}

impl BuildXML for Color {
    fn build_to<W: Write>(
        &self,
        stream: crate::docx::xml::writer::EventWriter<W>,
    ) -> crate::docx::xml::writer::Result<crate::docx::xml::writer::EventWriter<W>> {
        XMLBuilder::from(stream)
            .color_with_theme(
                &self.val,
                self.theme_color.as_deref(),
                self.theme_shade.as_deref(),
                self.theme_tint.as_deref(),
            )?
            .into_inner()
    }
}

