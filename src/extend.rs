//! Extension points: custom fenced-block renderers and custom HTML tag styles.
//!
//! Both are chosen by whoever *builds the [`Config`](crate::Config)*, never by
//! the Markdown author, so they are safe to use with untrusted documents. They
//! cannot reach around the converter's safety checks: a renderer only returns
//! PNG bytes (size, pixel and format checks still apply), and a tag style only
//! chooses among a fixed set of formatting properties.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// Error type returned by a [`FenceRenderer`]: anything printable.
pub type FenceError = Box<dyn std::error::Error + Send + Sync>;

/// Turns the text of a fenced code block into a PNG picture.
///
/// Register one per fence language with
/// [`Config::with_fence_renderer`](crate::Config::with_fence_renderer):
///
/// ```
/// use silvercity_oss::{markdown_to_docx, Config};
///
/// # fn draw(_: &str) -> Vec<u8> { let mut p = Vec::new(); image::RgbaImage::new(40, 20)
/// #     .write_to(&mut std::io::Cursor::new(&mut p), image::ImageFormat::Png).unwrap(); p }
/// let cfg = Config::default().with_fence_renderer("chart", |source: &str| Ok(draw(source)));
/// let docx = markdown_to_docx("```chart\n1 2 3\n```\n", &cfg).unwrap();
/// assert!(docx.starts_with(b"PK"));
/// ```
///
/// A renderer registered for `mermaid` or `dolphin` replaces the external
/// command for that language, which is how to plug in an in-process renderer.
///
/// # Contract
///
/// * Return the bytes of a PNG. The result is held to the same limits as any
///   other image (`Limits::max_image_bytes`, pixel budget, format check).
/// * The call is synchronous and **cannot be interrupted**: the converter checks
///   the clock only after it returns. A renderer that may run for a long time
///   on hostile input must bound itself.
/// * A panic is caught and reported as an error; an `Err` aborts the conversion
///   with [`CrateError::ExternalToolFailed`](crate::CrateError).
/// * It is called at most once per distinct block per conversion, on the
///   calling thread, and must be `Send + Sync` because a [`Config`](crate::Config)
///   may be shared between threads.
pub trait FenceRenderer: Send + Sync {
    /// Render `source` (the text between the fences) to PNG bytes.
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError>;
}

impl<F> FenceRenderer for F
where
    F: Fn(&str) -> Result<Vec<u8>, FenceError> + Send + Sync,
{
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError> {
        self(source)
    }
}

/// The renderers registered on a [`Config`](crate::Config), by lower-case
/// fence language.
#[derive(Clone, Default)]
pub struct FenceRenderers(pub(crate) BTreeMap<String, Arc<dyn FenceRenderer>>);

impl FenceRenderers {
    /// The renderer for fence language `lang` (compared case-insensitively).
    pub fn get(&self, lang: &str) -> Option<&Arc<dyn FenceRenderer>> {
        self.0.get(&lang.to_ascii_lowercase())
    }

    /// Languages with a renderer, sorted.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// True when no renderer is registered.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of registered renderers.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for FenceRenderers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

/// How the contents of a block-level custom tag are laid out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct BlockStyle {
    /// Paragraph background as six hex digits, e.g. `FFF4CE`.
    pub fill: Option<String>,
    /// Colour of a bar down the left edge, six hex digits.
    pub bar_color: Option<String>,
    /// Extra left indent in twips (1/1440 inch), at most 4320.
    pub indent_twips: u32,
    /// Bold text placed before the content, e.g. `Note: `.
    pub label: Option<String>,
}

impl BlockStyle {
    /// An empty block style: contents start a new paragraph and nothing else.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the paragraph background.
    pub fn fill(mut self, hex: impl Into<String>) -> Self {
        self.fill = Some(hex.into());
        self
    }

    /// Set the left bar colour.
    pub fn bar_color(mut self, hex: impl Into<String>) -> Self {
        self.bar_color = Some(hex.into());
        self
    }

    /// Set the extra left indent in twips.
    pub fn indent_twips(mut self, twips: u32) -> Self {
        self.indent_twips = twips;
        self
    }

    /// Set a bold label placed before the content.
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }
}

/// How an HTML tag that is not part of the built-in set (or one that is, to
/// override it) is rendered. Register with
/// [`Config::with_html_tag`](crate::Config::with_html_tag):
///
/// ```
/// use silvercity_oss::{markdown_to_docx, BlockStyle, Config, TagStyle};
///
/// let cfg = Config::untrusted()
///     .with_html_tag("mark", TagStyle::new().highlight("FFFF00"))
///     .with_html_tag(
///         "callout",
///         TagStyle::new().block(BlockStyle::new().fill("FFF4CE").bar_color("E0A800").label("Note: ")),
///     );
/// let docx = markdown_to_docx("<callout>mind the <mark>gap</mark></callout>\n", &cfg).unwrap();
/// assert!(docx.starts_with(b"PK"));
/// ```
///
/// Tag names are lower case letters, digits and `-`, starting with a letter
/// (`call-out` is fine). The tag's attributes are ignored, so a document cannot
/// pick colours or sizes: it can only turn the styles *you* defined on or off.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct TagStyle {
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underline: bool,
    /// Struck-through text.
    pub strike: bool,
    /// Monospace text (the inline code style).
    pub monospace: bool,
    /// Text colour as six hex digits.
    pub color: Option<String>,
    /// Text background (highlight) as six hex digits.
    pub highlight: Option<String>,
    /// When set, the tag is a block: its contents start a new paragraph, laid
    /// out as described. When `None` the tag is inline.
    pub block: Option<BlockStyle>,
}

impl TagStyle {
    /// A style that changes nothing (the tag is accepted and its text shown).
    pub fn new() -> Self {
        Self::default()
    }

    /// Bold text.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Italic text.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Underlined text.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Struck-through text.
    pub fn strike(mut self) -> Self {
        self.strike = true;
        self
    }

    /// Monospace text.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    /// Text colour, six hex digits.
    pub fn color(mut self, hex: impl Into<String>) -> Self {
        self.color = Some(hex.into());
        self
    }

    /// Text background, six hex digits.
    pub fn highlight(mut self, hex: impl Into<String>) -> Self {
        self.highlight = Some(hex.into());
        self
    }

    /// Make the tag a block with the given layout.
    pub fn block(mut self, style: BlockStyle) -> Self {
        self.block = Some(style);
        self
    }
}

pub(crate) fn is_hex_color(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Check a tag name and style; the error names the problem.
pub(crate) fn validate_tag(name: &str, style: &TagStyle) -> Result<(), String> {
    let ok_name = name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok_name {
        return Err(format!(
            "html tag name `{name}` must be 1 to 32 lower-case letters, digits or `-`, starting with a letter"
        ));
    }
    if crate::html::is_hidden(name) {
        return Err(format!(
            "html tag `{name}` can never be shown (its content is always dropped) and cannot be restyled"
        ));
    }
    let colors = [
        ("color", style.color.as_deref()),
        ("highlight", style.highlight.as_deref()),
        ("block fill", style.block.as_ref().and_then(|b| b.fill.as_deref())),
        (
            "block bar colour",
            style.block.as_ref().and_then(|b| b.bar_color.as_deref()),
        ),
    ];
    for (what, value) in colors {
        if let Some(v) = value {
            if !is_hex_color(v) {
                return Err(format!(
                    "html tag `{name}`: {what} `{v}` must be six hex digits like FFF4CE"
                ));
            }
        }
    }
    if let Some(b) = &style.block {
        if b.indent_twips > 4320 {
            return Err(format!(
                "html tag `{name}`: indent {} twips exceeds 4320",
                b.indent_twips
            ));
        }
        if let Some(l) = &b.label {
            if l.chars().count() > 64 || l.chars().any(char::is_control) {
                return Err(format!(
                    "html tag `{name}`: label must be at most 64 printable characters"
                ));
            }
        }
    }
    Ok(())
}

/// Check a fence language name.
pub(crate) fn validate_fence_language(lang: &str) -> Result<(), String> {
    let ok = !lang.is_empty()
        && lang.len() <= 32
        && lang
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '+' | '#' | '.'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "fence language `{lang}` must be 1 to 32 lower-case letters, digits or `-_+#.`"
        ))
    }
}
