use std::io::Write;

use crate::docx::documents::BuildXML;
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
