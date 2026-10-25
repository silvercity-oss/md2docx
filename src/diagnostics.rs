//! Structured findings about what the converter could not carry over faithfully.
//!
//! Every [`Diagnostic`] has a stable [`Code`], a [`Severity`], a message for people and, where the
//! finding belongs to a place in the document, a [`SourceSpan`] in the **original** Markdown the caller
//! passed in (not the text after front matter was blanked or diagrams were rewritten).
//!
//! A host that keeps its own preview or validation can compare against these, and one that must
//! not deliver a degraded document sets [`LossPolicy::Reject`](crate::LossPolicy) so that any
//! [`Severity::Warning`] fails the conversion instead.

use std::fmt;
use std::ops::Range;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// Informational: nothing was lost (for example, front matter was read and not rendered).
    Info,
    /// Content was dropped, degraded or left unconverted. [`LossPolicy::Reject`](crate::LossPolicy)
    /// refuses a document that has any.
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
        })
    }
}

macro_rules! codes {
    ($( $(#[$doc:meta])* $variant:ident => ($id:literal, $sev:ident, $summary:literal), )+) => {
        /// Stable identifier of a kind of finding. The string form ([`Code::as_str`]) never
        /// changes once released; new codes may be added.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Code {
            $( $(#[$doc])* $variant, )+
        }

        impl Code {
            /// Every code, in declaration order.
            pub const ALL: &'static [Code] = &[ $( Code::$variant, )+ ];

            /// The stable identifier, such as `image.remote`.
            pub const fn as_str(self) -> &'static str {
                match self { $( Code::$variant => $id, )+ }
            }

            /// The severity every finding with this code has.
            pub const fn severity(self) -> Severity {
                match self { $( Code::$variant => Severity::$sev, )+ }
            }

            /// One line saying what the code means.
            pub const fn summary(self) -> &'static str {
                match self { $( Code::$variant => $summary, )+ }
            }
        }
    };
}

codes! {
    /// An `http(s)` or `data:` image was not fetched or decoded; its alt text is shown.
    ImageRemote => ("image.remote", Warning, "a remote or inline image was not embedded"),
    /// A local image was not read because file access is off and the host supplied no bytes.
    ImageDisabled => ("image.disabled", Warning, "a local image was not embedded because image access is disabled"),
    /// A local image file could not be read (missing, unreadable, not a regular file).
    ImageUnreadable => ("image.unreadable", Warning, "a local image could not be read"),
    /// The bytes are not a PNG, JPEG or GIF this build can decode, or are corrupt.
    ImageUnsupported => ("image.unsupported", Warning, "an image could not be embedded (unsupported or corrupt)"),
    /// A footnote reference inside a footnote is left as written.
    FootnoteNested => ("footnote.nested", Warning, "a footnote reference inside a footnote was left as text"),
    /// `[^label]` with no definition is left as written.
    FootnoteUndefined => ("footnote.undefined", Info, "a footnote reference has no definition"),
    /// A footnote definition that nothing references is not rendered.
    FootnoteUnreferenced => ("footnote.unreferenced", Warning, "a footnote definition is never referenced, so its content was dropped"),
    /// A link destination that is not allowed; the text stays, the target is dropped.
    LinkDropped => ("link.dropped", Warning, "a link destination is not allowed, so only its text was kept"),
    /// `[x](#anchor)` where no heading has that anchor.
    LinkAnchorMissing => ("link.anchor-missing", Warning, "a link points at a heading anchor that does not exist"),
    /// A link target was shortened to make it safe (a `mailto:` field outside the allow-list).
    LinkModified => ("link.modified", Warning, "a link target was shortened: parts of it are not allowed"),
    /// A raw HTML `<a>` that was not turned into a hyperlink.
    LinkHtmlDropped => ("link.html-dropped", Warning, "a raw HTML link was not turned into a hyperlink"),
    /// List nesting deeper than the document format's levels was flattened to the deepest level.
    ListDepthClamped => ("list.depth-clamped", Warning, "list nesting deeper than the supported levels was flattened"),
    /// A table was dropped (empty, or nested too deeply).
    TableDropped => ("table.dropped", Warning, "a table was dropped"),
    /// A raw HTML element whose content is never shown (script, style, iframe, ...).
    HtmlUnsupported => ("html.unsupported", Warning, "a raw HTML element is unsupported and its content was dropped"),
    /// A `mermaid`/`dolphin` fence was left as a code block because rendering is disabled.
    DiagramNotRendered => ("diagram.not-rendered", Warning, "a diagram was left as a code block (rendering is disabled)"),
    /// A diagram renderer failed and the block was kept as a code block.
    DiagramKeptAsCode => ("diagram.kept-as-code", Warning, "a diagram could not be rendered and was kept as a code block"),
    /// A LaTeX command the converter does not know is shown as literal text.
    MathUnknownCommand => ("math.unknown-command", Warning, "an equation uses a LaTeX command that is not supported"),
    /// Characters that cannot appear in a Word document were removed.
    InputControlCharacters => ("input.control-characters", Warning, "characters that cannot appear in a document were removed"),
    /// A YAML/TOML metadata block at the start was read for the title and author and not rendered.
    InputFrontMatter => ("input.front-matter", Info, "front matter was read and not rendered"),
    /// More findings than the report holds; the rest were counted, not stored.
    ReportTruncated => ("report.truncated", Warning, "further findings were not recorded"),
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A place in the original Markdown.
///
/// `start` and `end` are byte offsets (`end` exclusive) into the string passed to the conversion;
/// lines and columns are 1-based, columns count characters. The span is the smallest range of the
/// original source that produced the finding: when the converter rewrote part of the document
/// (a diagram fence became a picture, say), it is the whole original block.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceSpan {
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
    /// Line of `start`.
    pub line: usize,
    /// Column of `start`.
    pub column: usize,
    /// Line of the last character.
    pub end_line: usize,
    /// Column just past the last character.
    pub end_column: usize,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Which kind of finding this is.
    pub code: Code,
    /// Copied from [`Code::severity`].
    pub severity: Severity,
    /// What happened, for people. Not stable: match on `code`.
    pub message: String,
    /// Where in the original Markdown, if the finding belongs to a place.
    pub span: Option<SourceSpan>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]", self.severity, self.code)?;
        if let Some(s) = &self.span {
            write!(f, " at line {}, column {}", s.line, s.column)?;
        }
        write!(f, ": {}", self.message)
    }
}

/// What to do when the converter could not carry something over faithfully (any
/// [`Severity::Warning`] finding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LossPolicy {
    /// Produce the document anyway, degrading each construct as documented (an unembeddable image
    /// becomes its alt text, a disallowed link keeps its text, ...). The findings are reported by
    /// [`convert`](crate::convert) and [`analyze`](crate::analyze), but the byte-returning
    /// functions return success. This is the historical behaviour and the default.
    #[default]
    Allow,
    /// Fail with [`CrateError::LossyContent`](crate::CrateError) instead of returning a document
    /// that differs from its source. Nothing is written.
    Reject,
}

/// Counts of what the conversion produced, for cross-checking against a host's own preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stats {
    /// Headings.
    pub headings: usize,
    /// Tables (Markdown and HTML), including nested ones.
    pub tables: usize,
    /// Embedded pictures: images, rendered diagrams and image-mode formulas.
    pub pictures: usize,
    /// Native (OMML) equations.
    pub equations: usize,
    /// Footnotes that appear in the document.
    pub footnotes: usize,
    /// Hyperlinks (external and to headings).
    pub hyperlinks: usize,
    /// Code blocks.
    pub code_blocks: usize,
}

/// Diagnostics and statistics of one conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Report {
    /// The findings, in document order (those without a place first).
    pub diagnostics: Vec<Diagnostic>,
    /// What was produced.
    pub stats: Stats,
}

impl Report {
    /// True when any finding is a [`Severity::Warning`].
    pub fn has_warnings(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity >= Severity::Warning)
    }
}

// ----------------------------------------------------------------------------------- internals

/// Which text a raw range refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Loc {
    /// Not tied to a place.
    None,
    /// Already a range of the original text.
    Original(Range<usize>),
    /// The text after control characters and front matter were dealt with, before diagrams and
    /// formulas were rewritten.
    Prepared(Range<usize>),
    /// The text the document builder parses.
    Rewritten(Range<usize>),
}

#[derive(Debug, Clone)]
pub(crate) struct Raw {
    pub code: Code,
    pub message: String,
    pub loc: Loc,
}

/// Findings collected while converting, capped so a hostile document cannot make the report the
/// largest thing in memory.
#[derive(Debug, Default, Clone)]
pub(crate) struct Collector {
    items: Vec<Raw>,
    suppressed: usize,
    /// (code, place) of what is already recorded: the same finding at the same place (a footnote
    /// referenced three times) is reported once.
    seen: std::collections::HashSet<(Code, u8, usize, usize)>,
}

const MAX_FINDINGS: usize = 500;

impl Collector {
    pub(crate) fn push(&mut self, code: Code, message: impl Into<String>, loc: Loc) {
        let key = match &loc {
            Loc::None => None,
            Loc::Original(r) => Some((0u8, r.start, r.end)),
            Loc::Prepared(r) => Some((1u8, r.start, r.end)),
            Loc::Rewritten(r) => Some((2u8, r.start, r.end)),
        };
        if self.items.len() >= MAX_FINDINGS {
            // Past the cap nothing more is stored, `seen` included.
            self.suppressed += 1;
            return;
        }
        if let Some((kind, start, end)) = key {
            if !self.seen.insert((code, kind, start, end)) {
                return;
            }
        }
        self.items.push(Raw {
            code,
            message: message.into(),
            loc,
        });
    }

    pub(crate) fn absorb(&mut self, other: Collector) {
        self.suppressed += other.suppressed;
        for r in other.items {
            self.push(r.code, r.message, r.loc);
        }
    }

    pub(crate) fn items(&self) -> &[Raw] {
        &self.items
    }

    pub(crate) fn suppressed(&self) -> usize {
        self.suppressed
    }
}

/// One replacement made to a text: `old` bytes became `new_len` bytes.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub old: Range<usize>,
    pub new_len: usize,
}

/// The edits of the successive stages, enough to point a position in the rewritten text back at
/// the original.
#[derive(Debug, Clone, Default)]
pub(crate) struct OffsetMap {
    /// original -> sanitised (invalid characters removed)
    pub sanitize: Vec<Edit>,
    /// sanitised -> prepared (byte-order mark and front matter blanked)
    pub blank: Vec<Edit>,
    /// prepared -> rewritten (diagrams and formulas replaced by picture references)
    pub rewrite: Vec<Edit>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bias {
    Start,
    End,
}

/// Map one position of an edited text back to the text before the edits. `edits` are sorted by
/// `old.start` and do not overlap.
fn unmap(edits: &[Edit], pos: usize, bias: Bias) -> usize {
    // `shift` is how far the edited text is ahead of the old text at the current edit.
    let mut shift: isize = 0;
    for e in edits {
        let new_start = (e.old.start as isize + shift) as usize;
        let new_end = new_start + e.new_len;
        if pos < new_start {
            break;
        }
        if pos == new_start {
            return e.old.start;
        }
        if pos < new_end {
            return if bias == Bias::Start { e.old.start } else { e.old.end };
        }
        if pos == new_end {
            return e.old.end;
        }
        shift += e.new_len as isize - (e.old.end - e.old.start) as isize;
    }
    (pos as isize - shift).max(0) as usize
}

fn unmap_range(edits: &[Edit], r: &Range<usize>) -> Range<usize> {
    let s = unmap(edits, r.start, Bias::Start);
    let e = unmap(edits, r.end, Bias::End).max(s);
    s..e
}

impl OffsetMap {
    /// The range of the original text that `loc` stands for.
    fn original(&self, loc: &Loc) -> Option<Range<usize>> {
        let prepared = match loc {
            Loc::None => return None,
            Loc::Original(r) => return Some(r.clone()),
            Loc::Rewritten(r) => unmap_range(&self.rewrite, r),
            Loc::Prepared(r) => r.clone(),
        };
        let sanitised = unmap_range(&self.blank, &prepared);
        Some(unmap_range(&self.sanitize, &sanitised))
    }
}

/// Turn raw findings into public ones, resolving each to a span of `original`.
pub(crate) fn finish(collector: &Collector, map: &OffsetMap, original: &str) -> Vec<Diagnostic> {
    // Byte offset of every line start, for line/column lookups.
    let mut starts = vec![0usize];
    starts.extend(original.match_indices('\n').map(|(i, _)| i + 1));
    let position = |offset: usize| {
        let offset = offset.min(original.len());
        let line = starts.partition_point(|&s| s <= offset) - 1;
        let mut at = offset;
        while !original.is_char_boundary(at) {
            at -= 1;
        }
        let column = original[starts[line]..at].chars().count() + 1;
        (line + 1, column)
    };
    let mut out: Vec<Diagnostic> = collector
        .items()
        .iter()
        .map(|raw| {
            let span = map.original(&raw.loc).map(|r| {
                let r = r.start.min(original.len())..r.end.min(original.len());
                let (line, column) = position(r.start);
                let (end_line, end_column) = position(r.end);
                SourceSpan {
                    start: r.start,
                    end: r.end,
                    line,
                    column,
                    end_line,
                    end_column,
                }
            });
            Diagnostic {
                code: raw.code,
                severity: raw.code.severity(),
                message: raw.message.clone(),
                span,
            }
        })
        .collect();
    // Document order; findings without a place (and equal places) keep the order they were made in.
    out.sort_by_key(|d| d.span.as_ref().map_or((0, 0), |s| (s.start, s.end)));
    if collector.suppressed() > 0 {
        out.push(Diagnostic {
            code: Code::ReportTruncated,
            severity: Code::ReportTruncated.severity(),
            message: format!("{} further findings were not recorded", collector.suppressed()),
            span: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(old: Range<usize>, new_len: usize) -> Edit {
        Edit { old, new_len }
    }

    #[test]
    fn positions_map_back_through_replacements() {
        // "aaaa[XXXXXX]bbbb" where the 6 bytes at 4..10 became 2 bytes.
        let edits = [e(4..10, 2)];
        let m = |p, b| unmap(&edits, p, b);
        assert_eq!(m(0, Bias::Start), 0);
        assert_eq!(m(3, Bias::End), 3);
        assert_eq!(m(4, Bias::Start), 4, "start of the replacement");
        assert_eq!(m(5, Bias::Start), 4, "inside: the whole original region");
        assert_eq!(m(5, Bias::End), 10);
        assert_eq!(m(6, Bias::Start), 10, "just after the replacement");
        assert_eq!(m(8, Bias::Start), 12, "later text is shifted back by the shrink");
    }

    #[test]
    fn ranges_compose_across_stages() {
        let map = OffsetMap {
            sanitize: vec![e(2..3, 0)],  // one char removed at 2
            blank: vec![e(0..6, 2)],     // 6 bytes became 2 newlines
            rewrite: vec![e(12..20, 4)], // 8 bytes became a 4-byte reference
        };
        // Rewritten 5..6 is two bytes after the blank region (rewritten = prepared here).
        // prepared 5 -> sanitised 9, which is original 10 because of the removed char.
        let r = map.original(&Loc::Rewritten(5..6)).unwrap();
        assert_eq!(r, 10..11);
        // Something inside the rewritten reference is the whole original block (12..20 + shifts).
        let whole = map.original(&Loc::Rewritten(13..15)).unwrap();
        assert_eq!(whole.end - whole.start, 8);
    }

    #[test]
    fn spans_report_lines_and_character_columns() {
        let original = "héllo\nsecond line\n";
        let mut c = Collector::default();
        c.push(Code::LinkDropped, "x", Loc::Prepared(7..13));
        let d = finish(&c, &OffsetMap::default(), original);
        let s = d[0].span.as_ref().unwrap();
        assert_eq!((s.line, s.column), (2, 1));
        assert_eq!((s.end_line, s.end_column), (2, 7));
        assert_eq!(&original[s.start..s.end], "second");
    }

    #[test]
    fn codes_are_unique_stable_and_documented() {
        let mut seen = std::collections::HashSet::new();
        for c in Code::ALL {
            assert!(seen.insert(c.as_str()), "duplicate code {c}");
            assert!(c.as_str().contains('.') && !c.summary().is_empty());
        }
        assert_eq!(Code::ImageRemote.as_str(), "image.remote");
        assert_eq!(Code::InputFrontMatter.severity(), Severity::Info);
    }

    #[test]
    fn the_collector_caps_memory_and_says_so() {
        let mut c = Collector::default();
        for _ in 0..(MAX_FINDINGS + 25) {
            c.push(Code::LinkDropped, "x", Loc::None);
        }
        let d = finish(&c, &OffsetMap::default(), "");
        assert_eq!(d.len(), MAX_FINDINGS + 1);
        assert_eq!(d.last().unwrap().code, Code::ReportTruncated);
    }
}
