//! Syntax highlighting for code blocks: token classes, themes, and the
//! [`Highlighter`] trait that turns source text into coloured spans.
//!
//! Highlighting is chosen by whoever builds the [`Config`](crate::Config), never
//! by the Markdown author. The built-in [`BuiltinHighlighter`] is a set of small,
//! hand-written lexers with no dependencies and linear running time, safe on
//! hostile text. For more accuracy, plug in a grammar-based engine such as
//! `syntect` or `tree-sitter-highlight` by implementing [`Highlighter`]; see the
//! README for working examples.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::error::{CrateError, Result};
use crate::extend::{is_hex_color, FenceError};

/// What a piece of source text is, for colouring.
///
/// The built-in lexers produce every class except `Operator` and `Punctuation`,
/// which are there for custom highlighters. A [`Theme`] decides how each looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TokenClass {
    /// Language keywords (`fn`, `class`, `SELECT`).
    Keyword,
    /// Keywords that change control flow (`if`, `return`, `for`).
    ControlKeyword,
    /// String and character literals.
    String,
    /// Comments.
    Comment,
    /// Numeric literals.
    Number,
    /// Function and method names, calls and definitions.
    Function,
    /// Type, class and trait names.
    Type,
    /// Variables and parameters.
    Variable,
    /// Language constants (`true`, `null`).
    Constant,
    /// Object keys and CSS properties.
    Property,
    /// Markup element names.
    Tag,
    /// Markup attribute names.
    Attribute,
    /// Added line in a diff.
    Inserted,
    /// Removed line in a diff.
    Deleted,
    /// Headings and file headers.
    Heading,
    /// Operators (not produced by the built-in lexers).
    Operator,
    /// Punctuation (not produced by the built-in lexers).
    Punctuation,
}

/// A byte range of the source with a class. Ranges outside any span are drawn
/// in the theme's plain foreground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Span {
    /// First byte (inclusive); a character boundary.
    pub start: usize,
    /// Last byte (exclusive); a character boundary.
    pub end: usize,
    /// How to colour it.
    pub class: TokenClass,
}

impl Span {
    /// A span covering `start..end`.
    pub fn new(start: usize, end: usize, class: TokenClass) -> Self {
        Span { start, end, class }
    }
}

/// Turns source text into spans.
///
/// ```
/// use md2docx::{markdown_to_docx, Config, FenceError, Highlighting, Highlighter, Span, Theme, TokenClass};
///
/// /// Colours every `TODO` in any language.
/// struct Todo;
/// impl Highlighter for Todo {
///     fn highlight(&self, _lang: &str, source: &str) -> Result<Option<Vec<Span>>, FenceError> {
///         Ok(Some(
///             source.match_indices("TODO").map(|(i, t)| Span::new(i, i + t.len(), TokenClass::Keyword)).collect(),
///         ))
///     }
/// }
/// let cfg = Config::default()
///     .with_highlighting(Highlighting::new(Theme::vscode_dark()).with_highlighter(Todo));
/// let docx = markdown_to_docx("```text\nTODO: write docs\n```\n", &cfg).unwrap();
/// assert!(docx.starts_with(b"PK"));
/// ```
///
/// # Contract
///
/// * Return `Ok(None)` for a language you do not handle (the block is drawn
///   plain), or the spans of the whole `source`.
/// * Spans must be sorted by `start`, must not overlap, and must lie on
///   character boundaries within `source`. A violation is reported as an error
///   ([`CrateError::ExternalToolFailed`](crate::CrateError)), not guessed at.
/// * `lang` is the lower-case first word of the fence's info string
///   (`` ```Rust,ignore `` gives `rust`).
/// * The call is synchronous and cannot be interrupted: bound your own running
///   time on hostile input. Sources longer than
///   [`Limits::max_highlight_bytes`](crate::Limits) are never passed to you.
/// * A panic is caught and reported as an error.
pub trait Highlighter: Send + Sync {
    /// Spans for `source`, or `None` if `lang` is not supported.
    fn highlight(&self, lang: &str, source: &str) -> std::result::Result<Option<Vec<Span>>, FenceError>;
}

impl<F> Highlighter for F
where
    F: Fn(&str, &str) -> std::result::Result<Option<Vec<Span>>, FenceError> + Send + Sync,
{
    fn highlight(&self, lang: &str, source: &str) -> std::result::Result<Option<Vec<Span>>, FenceError> {
        self(lang, source)
    }
}

/// The built-in lexers: Rust, Python, JavaScript, TypeScript, Java, C, C++, C#,
/// Go, Kotlin, Swift, Ruby, PHP, SQL, Bash, JSON, YAML, TOML, HTML/XML, CSS and
/// diffs. Regular lexers, not full grammars: they get keywords, strings,
/// comments, numbers and names right in ordinary code, and can be fooled by
/// unusual syntax (nested template strings, heredocs, macros that define
/// syntax). They allocate only the span list and run in linear time on any input.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinHighlighter;

impl BuiltinHighlighter {
    /// True when a built-in lexer exists for `lang` (aliases such as `rs`, `py`
    /// and `sh` are understood).
    pub fn supports(lang: &str) -> bool {
        crate::lexers::canonical(lang).is_some()
    }
}

impl Highlighter for BuiltinHighlighter {
    fn highlight(&self, lang: &str, source: &str) -> std::result::Result<Option<Vec<Span>>, FenceError> {
        Ok(crate::lexers::highlight(lang, source))
    }
}

/// Colour and emphasis for one [`TokenClass`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct TokenStyle {
    /// Text colour, six hex digits; `None` keeps the theme's foreground.
    pub color: Option<String>,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

impl TokenStyle {
    /// A style with just a colour.
    pub fn color(hex: impl Into<String>) -> Self {
        TokenStyle {
            color: Some(hex.into()),
            ..Self::default()
        }
    }

    /// Make it bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Make it italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

/// How highlighted code looks: the block's background and plain text colour, and
/// a [`TokenStyle`] per [`TokenClass`].
///
/// [`Theme::vscode_light`] and [`Theme::vscode_dark`] reproduce the token colours
/// of Visual Studio Code's *Light+* and *Dark+* themes (the colour values come
/// from microsoft/vscode, MIT licence). Build your own with [`Theme::new`] and
/// [`Theme::style`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Theme {
    /// Display name (at most 64 printable characters).
    pub name: String,
    /// Background of every code block, six hex digits.
    pub background: String,
    /// Colour of text no span covers, six hex digits.
    pub foreground: String,
    /// Colour of a thin frame around each code block, six hex digits. `None` frames
    /// the block in its own background colour, which only pads it.
    pub border: Option<String>,
    /// Styles by class; classes without an entry use the foreground.
    pub styles: BTreeMap<TokenClass, TokenStyle>,
}

impl Theme {
    /// An empty theme: every class is drawn in `foreground` until styled.
    pub fn new(name: impl Into<String>, background: impl Into<String>, foreground: impl Into<String>) -> Self {
        Theme {
            name: name.into(),
            background: background.into(),
            foreground: foreground.into(),
            border: None,
            styles: BTreeMap::new(),
        }
    }

    /// Set the style of `class`.
    pub fn style(mut self, class: TokenClass, style: TokenStyle) -> Self {
        self.styles.insert(class, style);
        self
    }

    /// Frame code blocks with a thin line of this colour.
    pub fn border(mut self, hex: impl Into<String>) -> Self {
        self.border = Some(hex.into());
        self
    }

    /// Visual Studio Code *Light+* (with a light-grey frame, since its background is white).
    pub fn vscode_light() -> Self {
        use TokenClass::*;
        Theme::new("VS Code Light+", "FFFFFF", "000000")
            .border("D0D0D0")
            .style(Keyword, TokenStyle::color("0000FF"))
            .style(ControlKeyword, TokenStyle::color("AF00DB"))
            .style(String, TokenStyle::color("A31515"))
            .style(Comment, TokenStyle::color("008000"))
            .style(Number, TokenStyle::color("098658"))
            .style(Function, TokenStyle::color("795E26"))
            .style(Type, TokenStyle::color("267F99"))
            .style(Variable, TokenStyle::color("001080"))
            .style(Constant, TokenStyle::color("0000FF"))
            .style(Property, TokenStyle::color("0451A5"))
            .style(Tag, TokenStyle::color("800000"))
            .style(Attribute, TokenStyle::color("E50000"))
            .style(Inserted, TokenStyle::color("098658"))
            .style(Deleted, TokenStyle::color("A31515"))
            .style(Heading, TokenStyle::color("800000").bold())
    }

    /// Visual Studio Code *Dark+*.
    pub fn vscode_dark() -> Self {
        use TokenClass::*;
        Theme::new("VS Code Dark+", "1E1E1E", "D4D4D4")
            .style(Keyword, TokenStyle::color("569CD6"))
            .style(ControlKeyword, TokenStyle::color("C586C0"))
            .style(String, TokenStyle::color("CE9178"))
            .style(Comment, TokenStyle::color("6A9955"))
            .style(Number, TokenStyle::color("B5CEA8"))
            .style(Function, TokenStyle::color("DCDCAA"))
            .style(Type, TokenStyle::color("4EC9B0"))
            .style(Variable, TokenStyle::color("9CDCFE"))
            .style(Constant, TokenStyle::color("569CD6"))
            .style(Property, TokenStyle::color("9CDCFE"))
            .style(Tag, TokenStyle::color("569CD6"))
            .style(Attribute, TokenStyle::color("9CDCFE"))
            .style(Inserted, TokenStyle::color("B5CEA8"))
            .style(Deleted, TokenStyle::color("CE9178"))
            .style(Heading, TokenStyle::color("569CD6").bold())
    }

    /// Check names and colours; the error says which is wrong.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.name.chars().count() > 64 || self.name.chars().any(char::is_control) {
            return Err("theme name must be at most 64 printable characters".into());
        }
        let mut colors = vec![("background", &self.background), ("foreground", &self.foreground)];
        if let Some(b) = &self.border {
            colors.push(("border", b));
        }
        for style in self.styles.values() {
            if let Some(c) = &style.color {
                colors.push(("token colour", c));
            }
        }
        for (what, value) in colors {
            if !is_hex_color(value) {
                return Err(format!(
                    "theme `{}`: {what} `{value}` must be six hex digits",
                    self.name
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn style_of(&self, class: TokenClass) -> Option<&TokenStyle> {
        self.styles.get(&class)
    }
}

/// Switches highlighting on: a [`Theme`] and the [`Highlighter`]s that colour
/// code blocks. With highlighting on, *every* code block (fenced, indented or
/// `<pre>`) takes the theme's background and foreground, so a dark theme is
/// consistent; blocks in a language no highlighter handles are drawn plain.
///
/// ```
/// use md2docx::{Config, Highlighting, Theme};
///
/// let cfg = Config::untrusted().with_highlighting(Highlighting::new(Theme::vscode_light()));
/// assert!(cfg.highlighting.is_some());
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct Highlighting {
    /// Colours.
    pub theme: Theme,
    /// Used for languages with no entry in [`Highlighting::languages`]
    /// (default [`BuiltinHighlighter`]).
    pub fallback: Arc<dyn Highlighter>,
    /// Highlighters for specific languages (lower case), which win over the fallback.
    pub languages: BTreeMap<String, Arc<dyn Highlighter>>,
}

impl Highlighting {
    /// The built-in lexers with `theme`.
    pub fn new(theme: Theme) -> Self {
        Highlighting {
            theme,
            fallback: Arc::new(BuiltinHighlighter),
            languages: BTreeMap::new(),
        }
    }

    /// Use `highlighter` for every language that has no specific entry, instead
    /// of the built-in lexers. It can delegate to [`BuiltinHighlighter`] itself.
    pub fn with_highlighter(mut self, highlighter: impl Highlighter + 'static) -> Self {
        self.fallback = Arc::new(highlighter);
        self
    }

    /// Use `highlighter` for fences tagged `lang` (case-insensitive).
    pub fn with_language(mut self, lang: impl AsRef<str>, highlighter: impl Highlighter + 'static) -> Self {
        self.languages
            .insert(lang.as_ref().to_ascii_lowercase(), Arc::new(highlighter));
        self
    }
}

impl fmt::Debug for Highlighting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Highlighting")
            .field("theme", &self.theme.name)
            .field("languages", &self.languages.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// The language of a fence's info string: its first word up to whitespace, `,`
/// or `{`, lower-cased. `None` for an empty or absent info string.
pub(crate) fn fence_language(info: &str) -> Option<String> {
    let word: String = info
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ',' && *c != '{')
        .collect();
    // Only plain language names are passed on (to highlighters and into messages).
    let word = word.to_lowercase();
    crate::extend::validate_fence_language(&word).ok().map(|()| word)
}

/// Highlight one code block: the spans, `None` for "draw it plain", or an error
/// when a user-supplied highlighter fails or returns invalid spans.
pub(crate) fn highlight_block(
    h: &Highlighting,
    lang: &str,
    source: &str,
    max_bytes: usize,
) -> Result<Option<Vec<Span>>> {
    if source.len() > max_bytes {
        return Ok(None);
    }
    let who = || format!("highlighter for `{lang}`");
    let highlighter = h.languages.get(lang).unwrap_or(&h.fallback);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| highlighter.highlight(lang, source)));
    let spans = match result {
        Ok(Ok(Some(spans))) => spans,
        Ok(Ok(None)) => return Ok(None),
        Ok(Err(e)) => {
            return Err(CrateError::ExternalToolFailed {
                program: who(),
                detail: e.to_string(),
            })
        }
        Err(_) => {
            return Err(CrateError::ExternalToolFailed {
                program: who(),
                detail: "the highlighter panicked".into(),
            })
        }
    };
    validate_spans(&spans, source).map_err(|detail| CrateError::ExternalToolFailed { program: who(), detail })?;
    Ok(Some(spans))
}

/// Sorted, non-overlapping, non-empty, inside `source`, on character boundaries.
pub(crate) fn validate_spans(spans: &[Span], source: &str) -> std::result::Result<(), String> {
    let mut previous_end = 0;
    for (i, s) in spans.iter().enumerate() {
        if s.start >= s.end || s.end > source.len() {
            return Err(format!(
                "span {i} ({}..{}) is empty or outside the {}-byte source",
                s.start,
                s.end,
                source.len()
            ));
        }
        if s.start < previous_end {
            return Err(format!(
                "span {i} ({}..{}) overlaps or precedes the previous span",
                s.start, s.end
            ));
        }
        if !source.is_char_boundary(s.start) || !source.is_char_boundary(s.end) {
            return Err(format!(
                "span {i} ({}..{}) is not on character boundaries",
                s.start, s.end
            ));
        }
        previous_end = s.end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_languages() {
        assert_eq!(fence_language("Rust"), Some("rust".into()));
        assert_eq!(fence_language("rust,ignore"), Some("rust".into()));
        assert_eq!(fence_language("js {1,3}"), Some("js".into()));
        assert_eq!(fence_language("  python title=x"), Some("python".into()));
        assert_eq!(fence_language(""), None);
        assert_eq!(fence_language("{.rust}"), None);
        assert_eq!(
            fence_language("\u{dc}n\u{ef}code<script>"),
            None,
            "unusual names are not passed on"
        );
        assert_eq!(fence_language(&"a".repeat(40)), None);
        assert_eq!(fence_language("c++"), Some("c++".into()));
        assert_eq!(fence_language("c#"), Some("c#".into()));
    }

    #[test]
    fn span_validation() {
        let src = "aé b";
        let ok = [
            Span::new(0, 1, TokenClass::Keyword),
            Span::new(1, 3, TokenClass::String),
        ];
        assert!(validate_spans(&ok, src).is_ok());
        for bad in [
            vec![Span::new(2, 3, TokenClass::Keyword)],  // inside é
            vec![Span::new(0, 0, TokenClass::Keyword)],  // empty
            vec![Span::new(0, 99, TokenClass::Keyword)], // outside
            vec![
                Span::new(2, 4, TokenClass::Keyword),
                Span::new(0, 1, TokenClass::Keyword),
            ], // unsorted
            vec![
                Span::new(0, 3, TokenClass::Keyword),
                Span::new(1, 4, TokenClass::Keyword),
            ], // overlap
        ] {
            assert!(validate_spans(&bad, src).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn built_in_themes_are_valid() {
        assert!(Theme::vscode_light().validate().is_ok());
        assert!(Theme::vscode_dark().validate().is_ok());
        assert!(Theme::new("x", "12345", "000000").validate().is_err());
        assert!(Theme::new("x", "000000", "000000")
            .style(TokenClass::Keyword, TokenStyle::color("red"))
            .validate()
            .is_err());
    }
}
