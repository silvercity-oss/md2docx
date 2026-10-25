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
/// * The call is synchronous and **cannot be interrupted** by the converter: it checks the
///   clock and the [`CancelToken`](crate::CancelToken) only after the call returns, and discards
///   a result that arrives late. A renderer that can run for a long time on hostile input must
///   bound itself, and should read the [`RenderContext`] it is given (override
///   [`FenceRenderer::render_with`]) to stop early when the conversion is cancelled or past its
///   deadline. A host that needs a *hard* deadline must run the whole conversion in a worker it
///   can terminate from outside.
/// * A panic is caught and reported as an error; an `Err` aborts the conversion
///   with [`CrateError::ExternalToolFailed`](crate::CrateError).
/// * The PNG is kept in memory; nothing is written to the scratch directory.
/// * It is called at most once per distinct block per conversion, on the
///   calling thread, and must be `Send + Sync` because a [`Config`](crate::Config)
///   may be shared between threads.
pub trait FenceRenderer: Send + Sync {
    /// Render `source` (the text between the fences) to PNG bytes.
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError>;

    /// Like [`FenceRenderer::render`], with the conversion's budgets and cancellation. The
    /// converter calls this method; the default ignores `ctx` and calls `render`.
    fn render_with(&self, source: &str, ctx: &RenderContext) -> Result<Vec<u8>, FenceError> {
        let _ = ctx;
        self.render(source)
    }
}

/// What a [`FenceRenderer`] may consult while it runs: the conversion's cancellation flag, its
/// deadline and the size ceilings its result will be held to. All of it is advisory (see the
/// contract on [`FenceRenderer`]).
#[derive(Debug, Clone)]
pub struct RenderContext {
    cancel: Option<crate::CancelToken>,
    deadline: Option<std::time::Instant>,
    max_image_bytes: u64,
    max_image_pixels: u64,
}

impl RenderContext {
    pub(crate) fn new(ctl: &crate::control::Ctl, cfg: &crate::Config) -> Self {
        RenderContext {
            cancel: ctl.cancel_token().cloned(),
            deadline: ctl.deadline(),
            max_image_bytes: cfg.limits.max_image_bytes,
            max_image_pixels: cfg.limits.max_image_pixels,
        }
    }

    /// A context with no cancellation, no deadline and no ceilings: for calling a renderer
    /// outside a conversion, such as in a unit test.
    pub fn unbounded() -> Self {
        RenderContext {
            cancel: None,
            deadline: None,
            max_image_bytes: u64::MAX,
            max_image_pixels: u64::MAX,
        }
    }

    /// True once the host cancelled the conversion.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(crate::CancelToken::is_cancelled)
    }

    /// When the conversion's time limit ends, if it has one.
    pub fn deadline(&self) -> Option<std::time::Instant> {
        self.deadline
    }

    /// Time left before the deadline (zero when it has passed); `None` without a deadline.
    pub fn remaining(&self) -> Option<std::time::Duration> {
        self.deadline
            .map(|d| d.saturating_duration_since(std::time::Instant::now()))
    }

    /// `Err` when the conversion was cancelled or is past its deadline. Call it in loops and
    /// `?` it to give up promptly.
    pub fn check(&self) -> Result<(), FenceError> {
        if self.is_cancelled() {
            return Err("the conversion was cancelled".into());
        }
        if self.remaining().is_some_and(|r| r.is_zero()) {
            return Err("the conversion's time limit has passed".into());
        }
        Ok(())
    }

    /// The most bytes the returned PNG may have ([`Limits::max_image_bytes`](crate::Limits)).
    pub fn max_image_bytes(&self) -> u64 {
        self.max_image_bytes
    }

    /// The most pixels the returned image may have ([`Limits::max_image_pixels`](crate::Limits)).
    pub fn max_image_pixels(&self) -> u64 {
        self.max_image_pixels
    }
}

impl<F> FenceRenderer for F
where
    F: Fn(&str) -> Result<Vec<u8>, FenceError> + Send + Sync,
{
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError> {
        self(source)
    }
}

/// Supplies the bytes of images the document refers to, chosen by the **host**.
///
/// This is how an embedder that stores its own images (an object store, a database, a set of
/// accepted attachments) gives the converter exact bytes without letting the document pick a
/// file path or a URL: the Markdown only names a key, and nothing is read from disk or fetched
/// from the network for it. Register one with
/// [`Config::with_image_resolver`](crate::Config::with_image_resolver). [`AssetMap`] covers the
/// common case of a fixed set.
///
/// ```
/// use silvercity_oss::{markdown_to_docx, AssetMap, Config};
///
/// # fn png() -> Vec<u8> { let mut p = Vec::new(); image::RgbaImage::new(40, 20)
/// #     .write_to(&mut std::io::Cursor::new(&mut p), image::ImageFormat::Png).unwrap(); p }
/// let mut assets = AssetMap::new();
/// assets.insert("accepted/diagram-7", png());
/// let cfg = Config::default().with_image_resolver(assets);
/// let docx = markdown_to_docx("![Login flow](accepted/diagram-7)", &cfg).unwrap();
/// assert!(docx.starts_with(b"PK"));
/// ```
///
/// # Contract
///
/// * `reference` is the destination exactly as written in the Markdown (`![alt](reference)`,
///   `<img src="reference">`). It is untrusted text: look it up, never open it as a path.
/// * Return `Ok(Some(bytes))` to supply the image, `Ok(None)` for "not mine" (the converter then
///   falls back to [`Config::image_access`](crate::Config) and, for the default
///   [`ImageAccess::None`](crate::ImageAccess), shows the alt text), or `Err` to fail the
///   conversion.
/// * The bytes are held to the same limits as any other image and fully decoded and
///   re-encoded; they are never trusted because the host supplied them.
/// * Called on the converting thread, at most once per distinct reference per conversion. A
///   panic is caught and reported as an error. The call is synchronous and cannot be
///   interrupted.
pub trait ImageResolver: Send + Sync {
    /// The bytes for `reference`, if the host has them.
    fn resolve(&self, reference: &str) -> Result<Option<Arc<[u8]>>, FenceError>;
}

impl<F> ImageResolver for F
where
    F: Fn(&str) -> Result<Option<Arc<[u8]>>, FenceError> + Send + Sync,
{
    fn resolve(&self, reference: &str) -> Result<Option<Arc<[u8]>>, FenceError> {
        self(reference)
    }
}

impl fmt::Debug for dyn ImageResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ImageResolver")
    }
}

/// A fixed set of images keyed by the reference the Markdown uses; an [`ImageResolver`].
#[derive(Debug, Clone, Default)]
pub struct AssetMap(BTreeMap<String, Arc<[u8]>>);

impl AssetMap {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Make `bytes` available as `![..](key)`. Replaces an earlier entry for `key`.
    pub fn insert(&mut self, key: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> &mut Self {
        self.0.insert(key.into(), bytes.into());
        self
    }

    /// Number of images.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when the map holds no image.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl ImageResolver for AssetMap {
    fn resolve(&self, reference: &str) -> Result<Option<Arc<[u8]>>, FenceError> {
        Ok(self.0.get(reference).cloned())
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
