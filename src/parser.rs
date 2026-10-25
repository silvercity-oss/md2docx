//! Stages 2 and 3: stream CommonMark events out of `pulldown-cmark` and map
//! them onto `docx-rs` elements.
//!
//! `docx-rs` cannot express OMML, so native equations are emitted as marker
//! runs and swapped for real `<m:oMath>` XML in the rendered `document.xml`
//! before the package is zipped (see [`inject_omml`]).

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use crate::docx::{
    AbstractNumbering, AlignmentType, BorderType, BreakType, Docx, Footer, Footnote, Hyperlink, HyperlinkType,
    IndentLevel, Level, LevelJc, LevelOverride, LevelText, LineSpacing, NumberFormat, Numbering, NumberingId,
    PageMargin, PageNum, Paragraph, ParagraphBorder, ParagraphBorderPosition, ParagraphBorders, Pic, Run, RunFonts,
    Shading, SpecialIndentType, Start, Style, StyleType, Table, TableCell, TableRow, VertAlignType, WidthType,
};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
};

use crate::config::{CodeBlockLayout, Config, ImageAccess, MathMode};
use crate::diagnostics::{Code, Collector, Loc, Report, Stats};
use crate::error::{CrateError, Result};
use crate::extend::TagStyle;
use crate::highlight::{self, Span, Theme, TokenClass};
use crate::html::{self, Piece};
use crate::images::{ImagePipeline, Outcome, Picture, Unsupported};
use crate::preprocess::{Asset, AssetKind, AssetSource, Preprocessed};
use crate::safety::{read_local_image, read_regular_file, safe_link, sanitize_xml_chars};

/// Parser options shared by Stage 1 (with `math`) and Stage 2 (without: by then
/// every formula has been rewritten, and any leftover `$` is literal text).
pub(crate) fn markdown_options(math: bool, sup_sub: bool) -> Options {
    let mut o = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES;
    if math {
        o |= Options::ENABLE_MATH;
    }
    if sup_sub {
        o |= Options::ENABLE_SUPERSCRIPT | Options::ENABLE_SUBSCRIPT;
    }
    o
}

/// Page layout derived from [`DocumentOptions`]; everything that depends on the
/// page size (image clamping, table width) reads it from here.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    page_w: u32,
    page_h: u32,
    margin: u32,
    /// Usable text width in twips.
    content_w: usize,
    /// Widest / tallest picture we place, in EMU (1 twip = 635 EMU).
    max_img_w: u128,
    max_img_h: u128,
}

impl Geometry {
    pub(crate) fn new(o: &crate::config::DocumentOptions) -> Self {
        let (page_w, page_h) = o.page.twips();
        let margin = o.margin_twips;
        let content_w = page_w.saturating_sub(2 * margin).max(1) as usize;
        let content_h = page_h.saturating_sub(2 * margin).max(1) as usize;
        Geometry {
            page_w,
            page_h,
            margin,
            content_w,
            max_img_w: content_w as u128 * 635,
            max_img_h: content_h as u128 * 635,
        }
    }
}

impl Default for Geometry {
    fn default() -> Self {
        Geometry::new(&crate::config::DocumentOptions::default())
    }
}

const EMU_PER_INCH: u64 = 914_400;
// docx-rs's default package already defines abstract numbering 1 / num 1, so
// ours start well clear of it.
const BULLET_ABSTRACT_ID: usize = 10;
const ORDERED_ABSTRACT_ID: usize = 11;
const BULLET_NUM_ID: usize = 10;
const HEADING_ABSTRACT_ID: usize = 12;
const HEADING_NUM_ID: usize = 9;
const MAX_LIST_LEVEL: usize = 8;

// ------------------------------------------------------------------ styles

fn fonts(name: &str) -> RunFonts {
    RunFonts::new().ascii(name).hi_ansi(name).east_asia(name).cs(name)
}

fn base_docx(o: &crate::config::DocumentOptions, g: &Geometry) -> Docx {
    let mut docx = Docx::new()
        .default_fonts(fonts(&o.body_font))
        .default_size(o.body_size_half_points as usize)
        .default_line_spacing(LineSpacing::new().after(120))
        .page_size(g.page_w, g.page_h)
        .page_margin(
            PageMargin::new()
                .top(g.margin as i32)
                .bottom(g.margin as i32)
                .left(g.margin as i32)
                .right(g.margin as i32),
        );
    let heading_sizes = [40usize, 32, 28, 26, 24, 22];
    for (i, size) in heading_sizes.iter().enumerate() {
        docx = docx.add_style(
            Style::new(format!("Heading{}", i + 1), StyleType::Paragraph)
                .name(format!("heading {}", i + 1))
                .size(*size)
                .bold()
                .color(o.heading_color.as_str())
                .outline_lvl(i),
        );
    }
    docx.add_style(
        Style::new("Hyperlink", StyleType::Character)
            .name("Hyperlink")
            .color(o.link_color.as_str())
            .underline("single"),
    )
    .add_style(
        Style::new("InlineCode", StyleType::Character)
            .name("Inline Code")
            .fonts(fonts(&o.code_font))
            .size(20),
    )
    .add_style(
        Style::new("CodeBlock", StyleType::Paragraph)
            .name("Code Block")
            .fonts(fonts(&o.code_font))
            .size(19)
            // Lines of one block sit directly under each other, with no gap.
            .line_spacing(LineSpacing::new().before(0).after(0)),
    )
    .add_style({
        // The small raised number in the text, and in front of the note itself.
        let mut style = Style::new("FootnoteReference", StyleType::Character).name("footnote reference");
        style.run_property = style.run_property.vert_align(VertAlignType::SuperScript);
        style
    })
}

/// Multilevel numbering for headings: `1.`, `1.1.`, `1.1.1.`, ... one level per heading level.
fn heading_numbering() -> AbstractNumbering {
    let mut numbering = AbstractNumbering::new(HEADING_ABSTRACT_ID);
    for lvl in 0..6usize {
        let text: String = (1..=lvl + 1).map(|i| format!("%{i}.")).collect();
        let text = text.trim_end_matches('.').to_string();
        let left = 567 + 284 * lvl as i32;
        numbering = numbering.add_level(
            Level::new(
                lvl,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(Some(left), Some(SpecialIndentType::Hanging(left)), None, None),
        );
    }
    numbering
}

fn numbering_definitions() -> (AbstractNumbering, AbstractNumbering) {
    let mut bullets = AbstractNumbering::new(BULLET_ABSTRACT_ID);
    let mut ordered = AbstractNumbering::new(ORDERED_ABSTRACT_ID);
    let glyphs = ["\u{2022}", "\u{25E6}", "\u{25AA}"];
    let formats = ["decimal", "lowerLetter", "lowerRoman"];
    for lvl in 0..=MAX_LIST_LEVEL {
        let left = 720 * (lvl as i32 + 1);
        bullets = bullets.add_level(
            Level::new(
                lvl,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new(glyphs[lvl % 3]),
                LevelJc::new("left"),
            )
            .indent(Some(left), Some(SpecialIndentType::Hanging(360)), None, None),
        );
        ordered = ordered.add_level(
            Level::new(
                lvl,
                Start::new(1),
                NumberFormat::new(formats[lvl % 3]),
                LevelText::new(format!("%{}.", lvl + 1)),
                LevelJc::new("left"),
            )
            .indent(Some(left), Some(SpecialIndentType::Hanging(360)), None, None),
        );
    }
    (bullets, ordered)
}

// ------------------------------------------------------------- builder

enum Child {
    Run(Run),
    Link(Hyperlink),
    /// A rendered PNG run; `kind` decides whether a lone one is centred.
    Picture {
        run: Run,
        kind: AssetKind,
    },
    /// An empty bookmark an HTML `id`/`name` anchor links can jump to.
    Bookmark(usize),
    /// Index into `Builder::omml`.
    Omml {
        slot: usize,
        display: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParaKind {
    Normal,
    Heading(u8),
}

struct ListCtx {
    num_id: usize,
    level: usize,
    item_open: bool,
    first_para: bool,
    task: Option<bool>,
}

struct LinkCtx {
    url: String,
    children: Vec<Child>,
    /// An internal target (heading or HTML anchor number) instead of `url`.
    bookmark: Option<usize>,
}

/// Width requested for an image by an HTML `width=` attribute.
#[derive(Clone, Copy)]
enum ImgWidth {
    /// CSS pixels.
    Px(u64),
    /// Percent of the text width.
    Percent(u64),
}

struct ImageCtx {
    dest: String,
    alt: String,
    title: String,
    width: Option<ImgWidth>,
}

#[derive(Default)]
struct TableState {
    aligns: Vec<Alignment>,
    rows: Vec<TableRow>,
    row_cells: Vec<TableCell>,
    cell: Option<TableCell>,
    cell_paras: usize,
    col: usize,
    in_head: bool,
    /// Built from HTML `<table>` tags rather than Markdown syntax.
    html: bool,
    /// Widest row seen so far (HTML tables have no header to count columns).
    max_cols: usize,
    /// Characters in the cell being built, and the longest cell seen in each column.
    cell_chars: usize,
    col_chars: Vec<usize>,
}

enum Block {
    Para(Box<Paragraph>),
    Table(Box<Table>),
}

struct OmmlSlot {
    xml: String,
    /// Decided when the paragraph is flushed: replace the whole `<w:p>`.
    block: bool,
    /// The marker is in `footnotes.xml` rather than `document.xml`.
    footnote: bool,
}

/// A footnote definition: its events with their source ranges, and the whole definition's range.
struct FootnoteDef<'a> {
    events: Vec<(Event<'a>, Range<usize>)>,
    range: Range<usize>,
}

/// A footnote definition that is still being read.
struct OpenDefinition<'a> {
    label: String,
    def: FootnoteDef<'a>,
}

/// `Parser` events with their source ranges and adjacent text merged (escapes, entities and stray
/// delimiters become one run, not one each), like `pulldown_cmark::TextMergeStream` does without
/// ranges.
struct MergedEvents<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> {
    inner: std::iter::Peekable<I>,
}

impl<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> Iterator for MergedEvents<'a, I> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let (event, mut range) = self.inner.next()?;
        let Event::Text(first) = event else {
            return Some((event, range));
        };
        let mut text = first.into_string();
        while matches!(self.inner.peek(), Some((Event::Text(_), _))) {
            if let Some((Event::Text(more), r)) = self.inner.next() {
                text.push_str(&more);
                range.end = r.end;
            }
        }
        Some((Event::Text(CowStr::from(text)), range))
    }
}

fn merged_events(markdown: &str, sup_sub: bool) -> MergedEvents<'_, impl Iterator<Item = (Event<'_>, Range<usize>)>> {
    MergedEvents {
        inner: Parser::new_ext(markdown, markdown_options(false, sup_sub))
            .into_offset_iter()
            .peekable(),
    }
}

struct Builder<'a> {
    pre: &'a Preprocessed,
    cfg: &'a Config,
    geo: Geometry,
    base_dir: PathBuf,
    nonce: String,
    /// Footnote bodies by label, collected before the main pass.
    footnotes: HashMap<String, FootnoteDef<'a>>,
    /// HTML anchors already given their bookmark.
    emitted_anchors: HashSet<usize>,
    /// Labels of the footnotes referenced from the main text.
    referenced: HashSet<String>,
    /// What could not be carried over, for the report.
    diags: Collector,
    /// Source range (in the rewritten Markdown) of the event being handled.
    at: Range<usize>,
    stats: Stats,
    /// True while building the content of a footnote (no pictures/OMML there).
    in_footnote: bool,
    /// Bytes of footnote content expanded so far (amplification guard).
    footnote_bytes: usize,
    /// Bytes of images embedded so far.
    images: ImagePipeline,
    /// The host resolver's answers by key, so each is asked for once.
    resolved: HashMap<String, Option<Arc<[u8]>>>,
    html: Option<String>,
    /// The open code buffer came from an HTML `<pre>`.
    html_pre: bool,
    /// Hidden element (`script`...) opened by inline HTML and not yet closed:
    /// text events are skipped until it closes or the paragraph ends.
    hidden: Option<&'static str>,
    /// The open link came from an HTML `<a>` (closed by `</a>` or the paragraph end).
    html_link: bool,
    /// Alignment from HTML `align=`/`<center>`: (element name, alignment), innermost last.
    align_stack: Vec<(String, AlignmentType)>,
    html_quotes: usize,
    html_lists: usize,
    underline: usize,
    sup: usize,
    sub: usize,
    mono: usize,
    def_depth: usize,
    /// Headings by anchor: `#slug` links resolve to the number of the heading's bookmark.
    anchors: Arc<HashMap<String, usize>>,
    /// Markdown headings started so far (the bookmark numbers).
    next_heading: usize,
    /// The heading whose paragraph is being built, if any.
    cur_heading: Option<usize>,
    /// HTML tables that were open when a Markdown table started inside one of their cells.
    outer_tables: Vec<TableState>,
    /// Custom HTML tags (from `Config::html_tags`) currently open, outermost first.
    custom_open: Vec<(&'a str, &'a TagStyle)>,
    /// Paragraphs, runs, cells and list instances built so far; each costs kilobytes of
    /// memory until the document is packaged, so the total is capped (`Limits::max_elements`).
    elements: usize,
    /// Set the moment `elements` passes the cap; reported by `feed`.
    budget_error: Option<CrateError>,

    blocks: Vec<Block>,
    cur: Option<Vec<Child>>,
    kind: ParaKind,
    bold: usize,
    italic: usize,
    strike: usize,
    link: Option<LinkCtx>,
    image: Option<ImageCtx>,
    /// Images opened inside the alt text of `image` (`![a ![b](x)](y)`).
    image_nesting: usize,
    code: Option<String>,
    /// Language of the fenced code block being read, for highlighting.
    code_lang: Option<String>,
    quote_depth: usize,
    lists: Vec<ListCtx>,
    ordered_numberings: Vec<Numbering>,
    next_num_id: usize,
    table: Option<TableState>,
    omml: Vec<OmmlSlot>,
}

enum UrlPiece<'t> {
    Text(&'t str),
    Url(&'t str),
}

/// Split `text` into plain text and bare web addresses (`http://…`, `https://…`, `www.…`), the
/// way GitHub does: an address starts at a word boundary, runs to whitespace or `<`, and does
/// not swallow the punctuation that ends a sentence or an unbalanced closing parenthesis.
fn split_bare_urls(text: &str) -> Vec<UrlPiece<'_>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut plain_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        let starts = rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("www.");
        let boundary = i == 0
            || !text[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '/');
        if starts && boundary {
            let mut end = rest.find(|c: char| c.is_whitespace() || c == '<').unwrap_or(rest.len());
            // Trailing punctuation belongs to the sentence, not the address.
            let (opens, mut closes) = rest[..end].bytes().fold((0usize, 0usize), |(o, c), b| {
                (o + usize::from(b == b'('), c + usize::from(b == b')'))
            });
            loop {
                let Some(last) = rest[..end].chars().next_back() else {
                    break;
                };
                let unbalanced_paren = last == ')' && closes > opens;
                if matches!(last, '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | '\'' | '"' | ';') || unbalanced_paren
                {
                    closes -= usize::from(last == ')');
                    end -= last.len_utf8();
                } else {
                    break;
                }
            }
            // `www.` alone, or a scheme with nothing after it, is not an address.
            let candidate = &rest[..end];
            let body = candidate.split_once("://").map_or(candidate, |(_, b)| b);
            if body.len() > 4 && body.contains('.') || candidate.starts_with("http") && body.len() > 1 {
                if plain_from < i {
                    out.push(UrlPiece::Text(&text[plain_from..i]));
                }
                out.push(UrlPiece::Url(candidate));
                i += end;
                plain_from = i;
                continue;
            }
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    if plain_from < text.len() {
        out.push(UrlPiece::Text(&text[plain_from..]));
    }
    out
}

/// Column widths (twips, summing to `total`) in proportion to how much text each column holds,
/// so a long description is not squeezed next to a two-character flag. Every column keeps at
/// least a fixed share, and one verbose column cannot take everything.
fn column_widths(total: usize, cols: usize, chars: &[usize]) -> Vec<usize> {
    const MIN_WEIGHT: usize = 6;
    const MAX_WEIGHT: usize = 48;
    let weights: Vec<usize> = (0..cols)
        .map(|i| chars.get(i).copied().unwrap_or(0).clamp(MIN_WEIGHT, MAX_WEIGHT))
        .collect();
    let sum: usize = weights.iter().sum::<usize>().max(1);
    let mut widths: Vec<usize> = weights.iter().map(|w| total * w / sum).collect();
    // Rounding: the last column takes what is left.
    let used: usize = widths.iter().sum();
    if let Some(last) = widths.last_mut() {
        *last += total.saturating_sub(used);
    }
    widths
}

/// Longest paragraph a code block is split into in `CodeBlockLayout::SingleParagraph`.
const SINGLE_PARAGRAPH_MAX_LINES: usize = 500;

/// How many tables may be open inside one another.
const MAX_NESTED_TABLES: usize = 8;

/// Widest indent written: the largest page Word accepts is 22 inches (31 680 twips).
const MAX_INDENT_TWIPS: usize = 31_680;

fn indent_twips(twips: usize) -> i32 {
    // MAX_INDENT_TWIPS is far below i32::MAX.
    i32::try_from(twips.min(MAX_INDENT_TWIPS)).unwrap_or(0)
}

/// One run of code text. Tabs become tab elements (a raw tab inside `<w:t>` is not reliably
/// shown by Word); a theme colours it by `class`.
fn code_run(text: &str, class: Option<TokenClass>, theme: Option<&Theme>) -> Run {
    let mut run = Run::new();
    for (i, part) in sanitize_xml_chars(text).split('\t').enumerate() {
        if i > 0 {
            run = run.add_tab();
        }
        if !part.is_empty() {
            run = run.add_text(part);
        }
    }
    if let Some(t) = theme {
        let style = class.and_then(|c| t.style_of(c));
        run = run.color(style.and_then(|s| s.color.as_deref()).unwrap_or(t.foreground.as_str()));
        if style.is_some_and(|s| s.bold) {
            run = run.bold();
        }
        if style.is_some_and(|s| s.italic) {
            run = run.italic();
        }
    }
    run
}

/// The runs of one code line: its text cut at the edges of the (validated, sorted) `spans`.
/// `next_span` is the first span that may still reach this or a later line.
fn code_line_runs(
    body: &str,
    (line_start, line_end): (usize, usize),
    spans: &[Span],
    next_span: &mut usize,
    theme: Option<&Theme>,
) -> Vec<Run> {
    while *next_span < spans.len() && spans[*next_span].end <= line_start {
        *next_span += 1;
    }
    let mut runs = Vec::new();
    let mut at = line_start;
    let mut k = *next_span;
    while at < line_end {
        match spans.get(k) {
            Some(span) if span.start < line_end => {
                if span.start > at {
                    runs.push(code_run(&body[at..span.start], None, theme));
                    at = span.start;
                }
                let end = span.end.min(line_end);
                runs.push(code_run(&body[at..end], Some(span.class), theme));
                at = end;
                if span.end > line_end {
                    break; // continues on the next line
                }
                k += 1;
            }
            _ => {
                runs.push(code_run(&body[at..line_end], None, theme));
                at = line_end;
            }
        }
    }
    *next_span = k;
    runs
}

/// What a link points at.
enum LinkTarget {
    /// An external address (already checked against the scheme allowlist).
    External(String),
    /// The heading with this number, by bookmark.
    Heading(usize),
}

/// Bookmark names must start with a letter and contain no spaces or hyphens.
/// Anchor numbers at or above this are HTML `id`/`name` targets, below it headings.
const HTML_ANCHOR_BASE: usize = 1 << 20;
/// Most HTML anchors one document may define.
const MAX_HTML_ANCHORS: usize = 10_000;

fn bookmark_name(n: usize) -> String {
    if n >= HTML_ANCHOR_BASE {
        format!("md_a{}", n - HTML_ANCHOR_BASE)
    } else {
        format!("md_h{n}")
    }
}

/// GitHub's heading anchors: lower case, spaces to hyphens, punctuation dropped.
fn github_slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' | '-' => Some('-'),
            '_' => Some('_'),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// Anchor name -> number of the heading, for every Markdown heading outside footnotes, in
/// document order. An explicit `{#id}` is used as written; repeated names get `-1`, `-2`...
/// A Markdown heading as the pre-pass saw it: level and plain text.
struct HeadingInfo {
    level: u8,
    text: String,
}

fn heading_anchors(markdown: &str, sup_sub: bool) -> (HashMap<String, usize>, Vec<HeadingInfo>) {
    let mut anchors = HashMap::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut in_definition = false;
    let mut open: Option<(Option<String>, String, u8)> = None;
    let mut headings: Vec<HeadingInfo> = Vec::new();
    let mut number = 0;
    let mut html_found: Vec<String> = Vec::new();
    for (event, _) in merged_events(markdown, sup_sub) {
        match event {
            Event::Start(Tag::FootnoteDefinition(_)) => in_definition = true,
            Event::End(TagEnd::FootnoteDefinition) => in_definition = false,
            _ if in_definition => {}
            Event::Start(Tag::Heading { id, level, .. }) => {
                open = Some((id.map(|i| i.to_string()), String::new(), heading_level(level)));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, text, _)) = open.as_mut() {
                    text.push_str(&t);
                }
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                for piece in html::tokenize(&h) {
                    if let Piece::Tag {
                        closing: false,
                        attrs,
                        name,
                    } = piece
                    {
                        let target = html::attr(&attrs, "id").or_else(|| {
                            if name == "a" {
                                html::attr(&attrs, "name")
                            } else {
                                None
                            }
                        });
                        if let Some(target) = target.filter(|t| !t.is_empty() && t.len() <= 200) {
                            let k = html_found.len();
                            if k < MAX_HTML_ANCHORS && !html_found.contains(&target) {
                                html_found.push(target);
                            }
                        }
                    }
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((id, text, level)) = open.take() {
                    headings.push(HeadingInfo {
                        level,
                        text: text.clone(),
                    });
                    let base: String = id.unwrap_or_else(|| github_slug(&text)).chars().take(200).collect();
                    let times = seen.entry(base.clone()).or_insert(0);
                    let name = if *times == 0 { base } else { format!("{base}-{times}") };
                    *times += 1;
                    anchors.entry(name).or_insert(number);
                    number += 1;
                }
            }
            _ => {}
        }
    }
    // Headings win a name they share with an HTML anchor.
    for (k, name) in html_found.into_iter().enumerate() {
        anchors.entry(name).or_insert(HTML_ANCHOR_BASE + k);
    }
    (anchors, headings)
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Put the footnote number in front of each note's first paragraph (docx-rs writes none, so
/// Word would show the note text without its number).
fn add_footnote_marks(xml: Vec<u8>) -> Vec<u8> {
    const MARK: &str = "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>\
                        <w:r><w:t xml:space=\"preserve\"> </w:t></w:r>";
    let Ok(text) = String::from_utf8(xml.clone()) else {
        return xml;
    };
    let mut out = String::with_capacity(text.len() + 256);
    let mut rest = text.as_str();
    while let Some(at) = rest.find("<w:footnote ") {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        let note_end = tail.find("</w:footnote>").unwrap_or(tail.len());
        let note = &tail[..note_end];
        // After the first paragraph's opening tag and its properties.
        let insert_at = note.find("<w:p ").and_then(|p| {
            let open_end = p + note[p..].find('>')? + 1;
            let after_props = if note[open_end..].starts_with("<w:pPr>") {
                open_end + note[open_end..].find("</w:pPr>")? + "</w:pPr>".len()
            } else {
                open_end
            };
            Some(after_props)
        });
        match insert_at {
            Some(i) if !note.contains("w:type=") => {
                out.push_str(&note[..i]);
                out.push_str(MARK);
                out.push_str(&note[i..]);
            }
            _ => out.push_str(note),
        }
        rest = &tail[note_end..];
    }
    out.push_str(rest);
    out.into_bytes()
}

/// `YYYY-MM-DDThh:mm:ssZ` for the current time (UTC), without a date library.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn marker(nonce: &str, slot: usize) -> String {
    format!("SILVERCITYOMML{nonce}X{slot}X")
}

/// Display size in EMU for a `w` x `h` pixel image at `dpi`, shrunk
/// (aspect ratio preserved) to fit the text area. Every step is checked;
/// `None` means the numbers do not fit, never a wrapped or panicking result.
fn display_size_emu(g: &Geometry, w: u32, h: u32, dpi: u32) -> Option<(u32, u32)> {
    let dpi = u128::from(dpi.max(1));
    let emu = u128::from(EMU_PER_INCH);
    let we = u128::from(w).checked_mul(emu)?.checked_div(dpi)?;
    let he = u128::from(h).checked_mul(emu)?.checked_div(dpi)?;
    fit_to_page(g, we, he)
}

/// Like [`display_size_emu`] but for a width the document asked for (EMU);
/// the height follows from the aspect ratio.
fn display_size_for_width(g: &Geometry, w: u32, h: u32, width_emu: u128) -> Option<(u32, u32)> {
    let he = width_emu
        .checked_mul(u128::from(h))?
        .checked_div(u128::from(w.max(1)))?;
    fit_to_page(g, width_emu, he)
}

/// Shrink (never grow) `we` x `he` EMU to the text area, keeping the aspect ratio.
fn fit_to_page(g: &Geometry, mut we: u128, mut he: u128) -> Option<(u32, u32)> {
    let max_w = g.max_img_w;
    if we > max_w {
        he = he.checked_mul(max_w)?.checked_div(we)?;
        we = max_w;
    }
    if he > g.max_img_h {
        we = we.checked_mul(g.max_img_h)?.checked_div(he)?;
        he = g.max_img_h;
    }
    // Never a zero-sized drawing; both values are now bounded by the page.
    Some((u32::try_from(we.max(1)).ok()?, u32::try_from(he.max(1)).ok()?))
}

/// How a picture is drawn: its resolution, a requested width, and its accessible description.
struct Placement<'t> {
    dpi: u32,
    width: Option<ImgWidth>,
    alt: &'t str,
    title: &'t str,
    kind: AssetKind,
}

/// A destination shortened for a message (they can be long, and they are untrusted text).
fn short(dest: &str) -> String {
    const MAX: usize = 80;
    let mut s: String = dest.chars().filter(|c| !c.is_control()).take(MAX).collect();
    if dest.chars().count() > MAX {
        s.push('\u{2026}');
    }
    s
}

/// Most distinct image keys whose resolver answers are remembered; beyond it each use asks again.
const MAX_RESOLVED_KEYS: usize = 4096;

/// Accessible description of a rendered asset: the diagram's or formula's own source, flattened
/// and shortened, so a screen reader user is not left with an unlabelled picture.
fn asset_description(asset: &Asset) -> String {
    const MAX_CHARS: usize = 250;
    // What the author wrote beats anything derived from the source.
    if let Some(alt) = asset.alt.as_deref().filter(|a| !a.trim().is_empty()) {
        return alt.to_string();
    }
    let flat: String = asset.original.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut text: String = flat.chars().take(MAX_CHARS).collect();
    if flat.chars().count() > MAX_CHARS {
        text.push('\u{2026}');
    }
    match asset.kind {
        AssetKind::Graph => format!("Diagram: {text}"),
        AssetKind::InlineMath | AssetKind::DisplayMath => format!("Equation: {text}"),
    }
}

/// Size a normalised picture for the page and wrap it as a `Pic`.
///
/// `width` is the width the document asked for, if any; otherwise the picture is drawn at
/// `dpi` and shrunk to fit the text area. `alt` and `title` become the drawing's accessible
/// description.
fn place_picture(
    g: &Geometry,
    pic: &Picture,
    dpi: u32,
    origin: &str,
    width: Option<ImgWidth>,
    alt: &str,
    title: &str,
) -> Result<Pic> {
    let (w, h) = (pic.width, pic.height);
    let requested = width.map(|wd| match wd {
        ImgWidth::Px(px) => u128::from(px) * 9525, // 1 CSS px = 9525 EMU
        ImgWidth::Percent(pct) => g.max_img_w * u128::from(pct.min(100)) / 100,
    });
    let sized = match requested {
        Some(width_emu) if width_emu > 0 => display_size_for_width(g, w, h, width_emu),
        _ => display_size_emu(g, w, h, dpi),
    };
    let (we, he) =
        sized.ok_or_else(|| CrateError::LimitExceeded(format!("image {origin} ({w}x{h} px) is out of range")))?;
    Ok(Pic::new_with_dimensions(Arc::clone(&pic.png), w, h)
        .size(we, he)
        .alt(sanitize_xml_chars(alt))
        .title(sanitize_xml_chars(title)))
}

impl<'a> Builder<'a> {
    fn new(pre: &'a Preprocessed, cfg: &'a Config, nonce: String) -> Self {
        Builder {
            pre,
            cfg,
            geo: Geometry::new(&cfg.document),
            base_dir: cfg.base_dir.clone().unwrap_or_else(|| PathBuf::from(".")),
            nonce,
            footnotes: HashMap::new(),
            emitted_anchors: HashSet::new(),
            referenced: HashSet::new(),
            diags: Collector::default(),
            at: 0..0,
            stats: Stats::default(),
            in_footnote: false,
            footnote_bytes: 0,
            images: ImagePipeline::new(),
            resolved: HashMap::new(),
            html: None,
            html_pre: false,
            hidden: None,
            html_link: false,
            align_stack: Vec::new(),
            html_quotes: 0,
            html_lists: 0,
            underline: 0,
            sup: 0,
            sub: 0,
            mono: 0,
            def_depth: 0,
            anchors: Arc::new(HashMap::new()),
            next_heading: 0,
            cur_heading: None,
            outer_tables: Vec::new(),
            custom_open: Vec::new(),
            elements: 0,
            budget_error: None,
            blocks: Vec::new(),
            cur: None,
            kind: ParaKind::Normal,
            bold: 0,
            italic: 0,
            strike: 0,
            link: None,
            image: None,
            image_nesting: 0,
            code: None,
            code_lang: None,
            quote_depth: 0,
            lists: Vec::new(),
            ordered_numberings: Vec::new(),
            next_num_id: BULLET_NUM_ID + 1,
            table: None,
            omml: Vec::new(),
        }
    }

    // ---- table of contents

    /// The paragraphs of a static table of contents: a title, then one linked line per heading.
    fn contents_blocks(
        &mut self,
        toc: &crate::config::TableOfContents,
        headings: &[HeadingInfo],
    ) -> Result<Vec<Block>> {
        const MAX_ENTRY_CHARS: usize = 200;
        let mut blocks = Vec::new();
        if !toc.title.is_empty() {
            let mut run = Run::new().add_text(sanitize_xml_chars(&toc.title)).bold();
            run.run_property = run.run_property.size(32);
            let title = Paragraph::new()
                .line_spacing(LineSpacing::new().before(0).after(200))
                .add_run(run);
            self.charge(2);
            blocks.push(Block::Para(Box::new(title)));
        }
        // Word numbers headings with one counter per level; the contents repeats those numbers.
        let mut counters = [0usize; 6];
        for (n, h) in headings.iter().enumerate() {
            let level = usize::from(h.level).clamp(1, 6);
            counters[level - 1] += 1;
            counters[level..].fill(0);
            if level > usize::from(toc.max_level) || h.text.trim().is_empty() {
                continue;
            }
            let mut label = String::new();
            if self.cfg.document.number_headings {
                let nums: Vec<String> = counters[..level].iter().map(usize::to_string).collect();
                label = format!("{} ", nums.join("."));
            }
            let text: String = h.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut text: String = text.chars().take(MAX_ENTRY_CHARS).collect();
            if h.text.chars().count() > MAX_ENTRY_CHARS {
                text.push('\u{2026}');
            }
            let link = Hyperlink::new(bookmark_name(n), HyperlinkType::Anchor)
                .add_run(Run::new().add_text(sanitize_xml_chars(&format!("{label}{text}"))));
            let entry = Paragraph::new()
                .line_spacing(LineSpacing::new().before(0).after(60))
                .indent(Some(indent_twips(360 * (level - 1))), None, None, None)
                .add_hyperlink(link);
            self.charge(3);
            if let Some(e) = self.budget_error.take() {
                return Err(e);
            }
            self.stats.hyperlinks += 1;
            blocks.push(Block::Para(Box::new(entry)));
        }
        if let Some(e) = self.budget_error.take() {
            return Err(e);
        }
        Ok(blocks)
    }

    // ---- findings

    /// Record something that could not be carried over, at the event being handled.
    fn note(&mut self, code: Code, message: impl Into<String>) {
        let at = self.at.clone();
        self.diags.push(code, message, Loc::Rewritten(at));
    }

    /// Footnote definitions that no reference reached are not part of the document.
    fn report_unreferenced_footnotes(&mut self) {
        let mut missing: Vec<(&String, &FootnoteDef<'a>)> = self
            .footnotes
            .iter()
            .filter(|(label, _)| !self.referenced.contains(*label))
            .collect();
        missing.sort_by_key(|(_, def)| def.range.start);
        let found: Vec<(String, Range<usize>)> = missing
            .into_iter()
            .map(|(label, def)| (label.clone(), def.range.clone()))
            .collect();
        for (label, range) in found {
            self.diags.push(
                Code::FootnoteUnreferenced,
                format!("footnote [^{label}] is defined but never referenced, so its content is not in the document"),
                Loc::Rewritten(range),
            );
        }
    }

    // ---- inline plumbing

    fn format(&self, mut run: Run) -> Run {
        if self.bold > 0 {
            run = run.bold();
        }
        if self.italic > 0 {
            run = run.italic();
        }
        if self.strike > 0 {
            run = run.strike();
        }
        if self.underline > 0 {
            run = run.underline("single");
        }
        if self.sup > 0 {
            run.run_property = run.run_property.vert_align(VertAlignType::SuperScript);
        } else if self.sub > 0 {
            run.run_property = run.run_property.vert_align(VertAlignType::SubScript);
        }
        if self.mono > 0 {
            run = run.style("InlineCode");
        }
        for (_, st) in &self.custom_open {
            if st.bold {
                run = run.bold();
            }
            if st.italic {
                run = run.italic();
            }
            if st.strike {
                run = run.strike();
            }
            if st.underline {
                run = run.underline("single");
            }
            if st.monospace {
                run = run.style("InlineCode");
            }
            if let Some(c) = &st.color {
                run = run.color(c.as_str());
            }
            if let Some(h) = &st.highlight {
                run = run.shading(Shading::new().fill(h.as_str()));
            }
        }
        run
    }

    /// Count `n` more elements against `Limits::max_elements`.
    fn charge(&mut self, n: usize) {
        self.elements = self.elements.saturating_add(n);
        if self.elements > self.cfg.limits.max_elements && self.budget_error.is_none() {
            self.budget_error = Some(CrateError::LimitExceeded(format!(
                "the document has more than {} paragraphs, runs and table cells (Limits::max_elements)",
                self.cfg.limits.max_elements
            )));
        }
    }

    fn push(&mut self, child: Child) {
        self.charge(1);
        if let Some(link) = self.link.as_mut() {
            link.children.push(child);
        } else {
            self.cur.get_or_insert_with(Vec::new).push(child);
        }
    }

    fn text(&mut self, t: &str) {
        self.count_cell_chars(t.chars().count());
        let run = self.format(Run::new().add_text(sanitize_xml_chars(t)));
        self.push(Child::Run(run));
    }

    /// Body text from the Markdown parser: like [`Builder::text`], and, when
    /// [`Config::autolink_bare_urls`] is on, bare `http://`, `https://` and `www.` addresses
    /// outside links become hyperlinks (only to destinations the link policy allows).
    fn prose(&mut self, t: &str) {
        if !self.cfg.autolink_bare_urls || self.link.is_some() {
            self.text(t);
            return;
        }
        for piece in split_bare_urls(t) {
            match piece {
                UrlPiece::Text(text) => self.text(text),
                UrlPiece::Url(raw) => {
                    let target = if raw.starts_with("www.") {
                        format!("https://{raw}")
                    } else {
                        raw.to_string()
                    };
                    match safe_link(&target, &self.cfg.allowed_link_schemes) {
                        Some(url) => {
                            self.link = Some(LinkCtx {
                                url: url.clone(),
                                children: Vec::new(),
                                bookmark: None,
                            });
                            self.text(raw);
                            if let Some(link) = self.link.take() {
                                self.finish_link(LinkTarget::External(url), link.children);
                            }
                        }
                        None => self.text(raw),
                    }
                }
            }
        }
    }

    fn start_para(&mut self, kind: ParaKind) {
        self.cur = Some(Vec::new());
        self.kind = kind;
    }

    fn count_cell_chars(&mut self, n: usize) {
        if let Some(t) = self.table.as_mut() {
            t.cell_chars = t.cell_chars.saturating_add(n);
        }
    }

    // ---- block plumbing

    fn emit(&mut self, p: Paragraph) {
        self.charge(1);
        if let Some(t) = self.table.as_mut() {
            if let Some(cell) = t.cell.take() {
                t.cell = Some(cell.add_paragraph(p));
                t.cell_paras += 1;
                return;
            }
        }
        // No open cell: content between the tags of an HTML table. Browsers move
        // it in front of the table, and so does this.
        self.blocks.push(Block::Para(Box::new(p)));
    }

    /// Resolve the children of a finished paragraph into a docx paragraph.
    fn flush(&mut self) {
        // An HTML `<a>` that was never closed ends with its paragraph.
        if self.html_link {
            self.close_html_link();
        }
        let Some(children) = self.cur.take() else { return };
        let kind = std::mem::replace(&mut self.kind, ParaKind::Normal);
        let heading = if matches!(kind, ParaKind::Heading(_)) {
            self.cur_heading.take()
        } else {
            None
        };
        if children.is_empty() {
            return;
        }

        // A paragraph holding only a graph or display formula is centred; a
        // lone native display equation becomes a real `<m:oMathPara>`.
        let solo = children.len() == 1;
        let centred = solo
            && matches!(
                &children[0],
                Child::Picture {
                    kind: AssetKind::Graph | AssetKind::DisplayMath,
                    ..
                }
            );
        let lone_display_equation = match (solo, children.first()) {
            (true, Some(Child::Omml { slot, display: true })) => Some(*slot),
            _ => None,
        };

        let mut p = Paragraph::new();
        if let ParaKind::Heading(n) = kind {
            p = p.style(&format!("Heading{n}"));
        }
        if let Some(n) = heading {
            p = p.add_bookmark_start(n, bookmark_name(n));
            // Only Markdown headings are numbered, so the numbers in the contents (which lists
            // exactly those) agree with Word's.
            if let (true, ParaKind::Heading(level)) = (self.cfg.document.number_headings, kind) {
                p = p.numbering(
                    NumberingId::new(HEADING_NUM_ID),
                    IndentLevel::new(usize::from(level) - 1),
                );
            }
        }

        let mut task = None;
        if kind == ParaKind::Normal && self.table.is_none() {
            if let Some(ctx) = self.lists.last_mut().filter(|c| c.item_open) {
                let left = 720 * (ctx.level as i32 + 1);
                task = ctx.task.take();
                if ctx.first_para && task.is_none() {
                    p = p.numbering(NumberingId::new(ctx.num_id), IndentLevel::new(ctx.level));
                } else if ctx.first_para {
                    p = p.indent(Some(left), Some(SpecialIndentType::Hanging(360)), None, None);
                } else {
                    p = p.indent(Some(left), None, None, None);
                }
                ctx.first_para = false;
            } else if self.quote_depth > 0 || self.def_depth > 0 {
                p = self.indented(p);
            }
        }

        // A task-list glyph shares the paragraph with the equation, so then it stays inline.
        let block_omml = lone_display_equation.filter(|_| task.is_none());
        if let Some(checked) = task {
            let glyph = if checked { "\u{2611} " } else { "\u{2610} " };
            p = p.add_run(Run::new().add_text(glyph));
        }

        if let Some(slot) = block_omml {
            self.omml[slot].block = true;
            p = p.add_run(Run::new().add_text(marker(&self.nonce, slot)));
        } else {
            for c in children {
                p = self.attach(p, c);
            }
        }
        if let Some(n) = heading {
            p = p.add_bookmark_end(n);
        }
        if centred {
            p = p.align(AlignmentType::Center);
        } else if let Some((_, a)) = self.align_stack.last() {
            p = p.align(*a);
        }
        p = self.apply_custom_block(p);
        if let Some(t) = self.table.as_ref() {
            let a = t.aligns.get(t.col).copied().unwrap_or(Alignment::None);
            p = match a {
                Alignment::Center => p.align(AlignmentType::Center),
                Alignment::Right => p.align(AlignmentType::Right),
                _ => p,
            };
        }
        self.emit(p);
    }

    /// Layout from the innermost open block-level custom tag, if any.
    fn apply_custom_block(&self, mut p: Paragraph) -> Paragraph {
        let Some(b) = self.custom_open.iter().rev().find_map(|(_, s)| s.block.as_ref()) else {
            return p;
        };
        if let Some(fill) = &b.fill {
            p.property = p.property.shading(Shading::new().fill(fill.as_str()));
        }
        // Inside a list item the numbering fixes the indent; leave it alone.
        let in_item = self.table.is_none() && self.lists.last().is_some_and(|c| c.item_open);
        if b.indent_twips > 0 && !in_item {
            let base = 360usize
                .saturating_mul(self.quote_depth)
                .saturating_add(720usize.saturating_mul(self.def_depth));
            let left = indent_twips(base.saturating_add(b.indent_twips as usize));
            p = p.indent(Some(left), None, None, None);
        }
        if let Some(color) = &b.bar_color {
            p.property = p.property.set_borders(
                ParagraphBorders::with_empty().set(
                    ParagraphBorder::new(ParagraphBorderPosition::Left)
                        .val(BorderType::Single)
                        .size(24)
                        .space(8)
                        .color(color.as_str()),
                ),
            );
        }
        p
    }

    /// A custom tag from `Config::html_tags` opens or closes.
    fn custom_tag(&mut self, key: &'a str, style: &'a TagStyle, closing: bool) {
        // Bounds the stack, and with it the work done per run, whatever the input.
        const MAX_OPEN: usize = 64;
        let is_block = style.block.is_some();
        if closing {
            // Flush while the style is still open so the paragraph gets its layout.
            if is_block {
                self.flush();
            }
            if let Some(i) = self.custom_open.iter().rposition(|(n, _)| *n == key) {
                self.custom_open.truncate(i);
            }
            return;
        }
        if is_block {
            self.flush();
        }
        if self.custom_open.len() < MAX_OPEN {
            self.custom_open.push((key, style));
        }
        if let Some(label) = style.block.as_ref().and_then(|b| b.label.as_deref()) {
            self.start_para(ParaKind::Normal);
            let run = self.format(Run::new().add_text(sanitize_xml_chars(label)).bold());
            self.push(Child::Run(run));
        }
    }

    fn attach(&mut self, p: Paragraph, c: Child) -> Paragraph {
        match c {
            Child::Run(r) | Child::Picture { run: r, .. } => p.add_run(r),
            Child::Link(l) => p.add_hyperlink(l),
            Child::Bookmark(n) => p.add_bookmark_start(n, bookmark_name(n)).add_bookmark_end(n),
            Child::Omml { slot, .. } => p.add_run(Run::new().add_text(marker(&self.nonce, slot))),
        }
    }

    /// Indent for block quotes (with a grey bar) and definition-list bodies.
    fn indented(&self, p: Paragraph) -> Paragraph {
        let left = 360usize
            .saturating_mul(self.quote_depth)
            .saturating_add(720usize.saturating_mul(self.def_depth));
        let mut p = p.indent(Some(indent_twips(left)), None, None, None);
        if self.quote_depth > 0 {
            p.property = p.property.set_borders(
                ParagraphBorders::with_empty().set(
                    ParagraphBorder::new(ParagraphBorderPosition::Left)
                        .val(BorderType::Single)
                        .size(18)
                        .space(8)
                        .color(self.cfg.document.quote_bar_color.as_str()),
                ),
            );
        }
        p
    }

    fn rule(&mut self) {
        let mut p = Paragraph::new();
        p.property = p.property.set_borders(
            ParagraphBorders::with_empty().set(
                ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                    .val(BorderType::Single)
                    .size(8)
                    .space(1)
                    .color("808080"),
            ),
        );
        self.emit(p);
    }

    fn code_block(&mut self, text: &str) {
        self.stats.code_blocks += 1;
        let body = text.strip_suffix('\n').unwrap_or(text);
        let lang = self.code_lang.take();
        let cfg: &'a Config = self.cfg;
        let theme = cfg.highlighting.as_ref().map(|h| &h.theme);
        let spans: Vec<Span> = match (cfg.highlighting.as_ref(), lang.as_deref()) {
            (Some(h), Some(l)) => match highlight::highlight_block(h, l, body, cfg.limits.max_highlight_bytes) {
                Ok(spans) => spans.unwrap_or_default(),
                Err(e) => {
                    self.budget_error.get_or_insert(e);
                    return;
                }
            },
            _ => Vec::new(),
        };
        let fill = theme.map_or(cfg.document.code_background.as_str(), |t| t.background.as_str());
        let left = match (self.lists.last(), self.quote_depth) {
            (Some(ctx), _) if ctx.item_open => Some(indent_twips(720 * (ctx.level + 1))),
            (_, q) if q > 0 => Some(indent_twips(360usize.saturating_mul(q))),
            _ => None,
        };
        // A thin frame when the theme asks for one (a white background would not show otherwise);
        // neighbouring paragraphs with equal borders are drawn as one box.
        let frame = theme.and_then(|t| t.border.as_deref());
        let new_paragraph = || {
            let mut p = Paragraph::new().style("CodeBlock");
            p.property = p.property.shading(Shading::new().fill(fill));
            if let Some(color) = frame {
                let side = |position| {
                    ParagraphBorder::new(position)
                        .val(BorderType::Single)
                        .size(4)
                        .space(4)
                        .color(color)
                };
                p.property = p.property.set_borders(
                    ParagraphBorders::with_empty()
                        .set(side(ParagraphBorderPosition::Top))
                        .set(side(ParagraphBorderPosition::Left))
                        .set(side(ParagraphBorderPosition::Bottom))
                        .set(side(ParagraphBorderPosition::Right)),
                );
            }
            if let Some(l) = left {
                p = p.indent(Some(l), None, None, None);
            }
            p
        };
        // Each line as (start, end) byte offsets in `body`, without the line ending.
        let mut lines: Vec<(usize, usize)> = Vec::new();
        let mut start = 0;
        for line in body.split('\n') {
            lines.push((start, start + line.trim_end_matches('\r').len()));
            start += line.len() + 1;
        }
        let mut next_span = 0;
        match cfg.document.code_blocks {
            CodeBlockLayout::LinePerParagraph => {
                for range in lines {
                    if self.budget_error.is_some() {
                        break;
                    }
                    let mut runs = code_line_runs(body, range, &spans, &mut next_span, theme);
                    if runs.is_empty() {
                        runs.push(Run::new());
                    }
                    self.charge(runs.len());
                    let mut p = new_paragraph();
                    for r in runs {
                        p = p.add_run(r);
                    }
                    self.emit(p);
                }
            }
            CodeBlockLayout::SingleParagraph => {
                // One paragraph per chunk of lines: LibreOffice's layout time grows with the
                // square of the lines in a paragraph (100k lines took 12 minutes).
                for chunk in lines.chunks(SINGLE_PARAGRAPH_MAX_LINES) {
                    if self.budget_error.is_some() {
                        break;
                    }
                    let mut p = new_paragraph();
                    let mut count = 0;
                    for (i, range) in chunk.iter().enumerate() {
                        if i > 0 {
                            // Every line break is a run of its own.
                            count += 1;
                            p = p.add_run(Run::new().add_break(BreakType::TextWrapping));
                        }
                        let runs = code_line_runs(body, *range, &spans, &mut next_span, theme);
                        count += runs.len();
                        for r in runs {
                            p = p.add_run(r);
                        }
                    }
                    self.charge(count);
                    self.emit(p);
                }
            }
        }
        // A thin empty paragraph, so a following code block is a box of its own instead of
        // merging with this one, and prose after the block has a gap.
        if self.budget_error.is_none() {
            let spacer = Paragraph::new()
                .line_spacing(LineSpacing::new().before(0).after(0))
                .run_property(crate::docx::RunProperty::new().size(8));
            self.emit(spacer);
        }
    }

    // ---- images and assets

    /// `[alt text]` stand-in for an image that is not embedded.
    fn placeholder(&mut self, alt: &str, fallback: &str) {
        let label = if alt.trim().is_empty() { fallback } else { alt };
        let run = self.format(Run::new().add_text(sanitize_xml_chars(&format!("[{label}]"))).italic());
        self.push(Child::Run(run));
    }

    /// Decode and normalise `bytes` and embed them, or show `alt` when they are not an image
    /// this crate embeds. Every image limit is enforced in [`ImagePipeline::normalize`].
    fn embed(&mut self, bytes: &[u8], origin: &str, how: Placement<'_>) -> Result<Option<Unsupported>> {
        let Placement {
            dpi,
            width,
            alt,
            title,
            kind,
        } = how;
        match self.images.normalize(&self.cfg.limits, &self.pre.ctl, bytes, origin)? {
            Outcome::Picture(pic) => {
                let pic = place_picture(&self.geo, &pic, dpi, origin, width, alt, title)?;
                self.push(Child::Picture {
                    run: Run::new().add_image(pic),
                    kind,
                });
                self.stats.pictures += 1;
                Ok(None)
            }
            Outcome::Unsupported(why) => Ok(Some(why)),
        }
    }

    /// Ask the host's [`ImageResolver`] (once per distinct key) for the bytes behind `key`.
    fn resolve_asset(&mut self, key: &str) -> Result<Option<Arc<[u8]>>> {
        let Some(resolver) = self.cfg.image_resolver.as_ref() else {
            return Ok(None);
        };
        if let Some(known) = self.resolved.get(key) {
            return Ok(known.clone());
        }
        self.pre.ctl.check()?;
        let failed = |detail: String| CrateError::ExternalToolFailed {
            program: "image resolver".into(),
            detail,
        };
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| resolver.resolve(key)))
            .map_err(|_| failed("the resolver panicked".into()))?
            .map_err(|e| failed(e.to_string()))?;
        self.pre.ctl.check()?;
        if self.resolved.len() < MAX_RESOLVED_KEYS {
            self.resolved.insert(key.to_string(), answer.clone());
        }
        Ok(answer)
    }

    fn image(&mut self, ctx: ImageCtx) -> Result<()> {
        if let Some(asset) = self.pre.assets.get(&ctx.dest) {
            let asset = asset.clone();
            return self.asset(&asset);
        }
        let dest = ctx.dest.as_str();
        // The host's own byte assets come first: the document only names a key.
        {
            if let Some(bytes) = self.resolve_asset(dest)? {
                let origin = format!("asset `{}`", sanitize_xml_chars(dest));
                if let Some(why) = self.embed(
                    &bytes,
                    &origin,
                    Placement {
                        dpi: 96,
                        width: ctx.width,
                        alt: &ctx.alt,
                        title: &ctx.title,
                        kind: AssetKind::Graph,
                    },
                )? {
                    self.note(
                        Code::ImageUnsupported,
                        format!("image `{}` was not embedded: {}", short(dest), why.describe()),
                    );
                    self.placeholder(&ctx.alt, "image");
                }
                return Ok(());
            }
        }
        let lower = dest.to_ascii_lowercase();
        // Remote and inline images are never fetched or decoded (no network
        // access by design); keep the alt text so nothing vanishes silently.
        let remote = lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("data:");
        if remote || self.cfg.image_access == ImageAccess::None {
            if remote {
                self.note(
                    Code::ImageRemote,
                    format!(
                        "image `{}` is remote or inline and is never fetched; its alt text is shown",
                        short(dest)
                    ),
                );
            } else {
                self.note(Code::ImageDisabled, format!("image `{}` was not embedded: local image access is disabled and the host supplied no bytes for it", short(dest)));
            }
            self.placeholder(&ctx.alt, dest);
            return Ok(());
        }
        let (path, bytes) = read_local_image(
            dest,
            &self.base_dir,
            self.cfg.image_access,
            self.cfg.limits.max_image_bytes,
        )?;
        let origin = path.display().to_string();
        if let Some(why) = self.embed(
            &bytes,
            &origin,
            Placement {
                dpi: 96,
                width: ctx.width,
                alt: &ctx.alt,
                title: &ctx.title,
                kind: AssetKind::Graph,
            },
        )? {
            // A format we cannot embed (SVG, WebP, corrupt data): keep the alt text.
            self.note(
                Code::ImageUnsupported,
                format!("image `{}` was not embedded: {}", short(dest), why.describe()),
            );
            self.placeholder(&ctx.alt, "image");
        }
        Ok(())
    }

    fn asset(&mut self, asset: &Asset) -> Result<()> {
        let dpi = match asset.kind {
            AssetKind::Graph => self.cfg.graphs.dpi,
            // Formulas are typeset at 12pt; body text is 11pt.
            _ => self.cfg.math.dpi.saturating_mul(12) / 11,
        };
        let alt = asset_description(asset);
        let (bytes, origin): (Arc<[u8]>, String) = match &asset.source {
            AssetSource::Png(path) => (
                read_regular_file(path, "rendered asset", self.cfg.limits.max_image_bytes)?.into(),
                path.display().to_string(),
            ),
            AssetSource::Bytes(bytes) => (Arc::clone(bytes), "renderer output".to_string()),
            AssetSource::Omml(body) => {
                let display = asset.kind == AssetKind::DisplayMath;
                self.omml.push(OmmlSlot {
                    xml: body.clone(),
                    block: false,
                    footnote: self.in_footnote,
                });
                self.push(Child::Omml {
                    slot: self.omml.len() - 1,
                    display,
                });
                self.stats.equations += 1;
                return Ok(());
            }
        };
        let title = asset.title.clone().unwrap_or_default();
        let how = Placement {
            dpi,
            width: None,
            alt: &alt,
            title: &title,
            kind: asset.kind,
        };
        match self.embed(&bytes, &origin, how)? {
            None => Ok(()),
            Some(_) => Err(CrateError::ExternalToolFailed {
                program: "renderer".into(),
                detail: format!("{origin} is not a decodable image"),
            }),
        }
    }

    // ---- lists and tables

    fn start_list(&mut self, first: Option<u64>) {
        if self.lists.len() > MAX_LIST_LEVEL {
            self.note(
                Code::ListDepthClamped,
                format!(
                    "list nesting deeper than {} levels is flattened to the deepest level",
                    MAX_LIST_LEVEL + 1
                ),
            );
        }
        let level = self.lists.len().min(MAX_LIST_LEVEL);
        let num_id = match first {
            None => BULLET_NUM_ID,
            Some(start) => {
                let id = self.next_num_id;
                self.next_num_id += 1;
                self.charge(1);
                // Word reads the start as a 32-bit number.
                let start = usize::try_from(start.min(i32::MAX as u64)).unwrap_or(1);
                self.ordered_numberings
                    .push(Numbering::new(id, ORDERED_ABSTRACT_ID).add_override(LevelOverride::new(level).start(start)));
                id
            }
        };
        self.lists.push(ListCtx {
            num_id,
            level,
            item_open: false,
            first_para: false,
            task: None,
        });
    }

    fn cell_start(&mut self) {
        if let Some(t) = self.table.as_mut() {
            t.cell = Some(TableCell::new());
            t.cell_paras = 0;
            t.cell_chars = 0;
        }
        self.start_para(ParaKind::Normal);
        if self.table.as_ref().is_some_and(|t| t.in_head) {
            self.bold += 1;
        }
    }

    fn cell_end(&mut self) {
        self.flush();
        let head = self.table.as_ref().is_some_and(|t| t.in_head);
        if head {
            self.bold = self.bold.saturating_sub(1);
        }
        if let Some(t) = self.table.as_mut() {
            if let Some(mut cell) = t.cell.take() {
                if t.cell_paras == 0 {
                    cell = cell.add_paragraph(Paragraph::new());
                }
                if head {
                    cell = cell.shading(Shading::new().fill("E7E6E6"));
                }
                t.row_cells.push(cell);
            }
            if t.col_chars.len() <= t.col {
                t.col_chars.resize(t.col + 1, 0);
            }
            t.col_chars[t.col] = t.col_chars[t.col].max(t.cell_chars);
            t.col += 1;
        }
        self.charge(1);
    }

    fn row_end(&mut self) {
        if let Some(t) = self.table.as_mut() {
            let cells = std::mem::take(&mut t.row_cells);
            t.rows.push(TableRow::new(cells));
            t.col = 0;
        }
    }

    // ---- raw HTML

    /// Render a piece of raw HTML found in the Markdown. Only presentation is
    /// mapped (bold/italic/underline/strike/sup/sub/code, line and paragraph
    /// breaks, headings, list bullets, rules, images); scripts, styles,
    /// comments and unknown elements contribute nothing but their text.
    fn html_fragment(&mut self, raw: &str, block: bool) -> Result<()> {
        if self.image.is_some() {
            return Ok(()); // inside Markdown alt text
        }
        if block {
            self.flush();
        }
        let mut dropped = Vec::new();
        let pieces = html::tokenize_reporting(raw, &mut dropped);
        for name in dropped {
            self.note(
                Code::HtmlUnsupported,
                format!("the content of a raw HTML <{name}> element is never shown and was dropped"),
            );
        }
        for piece in pieces {
            if self.budget_error.is_some() {
                break;
            }
            match piece {
                Piece::Text(t) if self.code.is_some() => {
                    // Inside <pre>: keep every space and newline.
                    if let Some(buf) = self.code.as_mut() {
                        buf.push_str(&t);
                    }
                }
                Piece::Text(t) => {
                    let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    let at_start = self.cur.as_ref().is_none_or(Vec::is_empty);
                    let mut s = collapsed;
                    if !at_start && t.starts_with(char::is_whitespace) {
                        s.insert(0, ' ');
                    }
                    if !at_start && t.ends_with(char::is_whitespace) && !s.ends_with(' ') {
                        s.push(' ');
                    }
                    if !s.trim().is_empty() {
                        self.text(&s);
                    }
                }
                Piece::Tag { name, closing, attrs } => self.html_tag(&name, closing, &attrs)?,
            }
        }
        if block {
            self.flush();
        }
        Ok(())
    }

    fn html_tag(&mut self, name: &str, closing: bool, attrs: &str) -> Result<()> {
        fn adjust(counter: &mut usize, closing: bool) {
            *counter = if closing {
                counter.saturating_sub(1)
            } else {
                counter.saturating_add(1)
            };
        }
        if !closing {
            self.html_anchor_target(name, attrs);
        }
        let cfg: &'a Config = self.cfg;
        if let Some((key, style)) = cfg.html_tags.get_key_value(name) {
            self.custom_tag(key, style, closing);
            return Ok(());
        }
        match name {
            "br" if !closing => self.push(Child::Run(Run::new().add_break(BreakType::TextWrapping))),
            "b" | "strong" => adjust(&mut self.bold, closing),
            "i" | "em" | "cite" | "dfn" | "var" => adjust(&mut self.italic, closing),
            "u" | "ins" => adjust(&mut self.underline, closing),
            "s" | "del" | "strike" => adjust(&mut self.strike, closing),
            "sup" => adjust(&mut self.sup, closing),
            "sub" => adjust(&mut self.sub, closing),
            "code" | "kbd" | "samp" | "tt" => adjust(&mut self.mono, closing),
            "a" => self.html_anchor(closing, attrs),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.block_boundary(name, closing, attrs);
                if !closing {
                    self.start_para(ParaKind::Heading(name.as_bytes()[1] - b'0'));
                }
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "aside" | "main" | "address"
            | "figure" | "figcaption" | "details" | "summary" | "center" | "dl" | "dt" | "dd" => {
                self.block_boundary(name, closing, attrs);
            }
            "hr" => {
                self.flush();
                self.rule();
            }
            "blockquote" => {
                self.block_boundary(name, closing, attrs);
                if !closing {
                    self.quote_depth += 1;
                    self.html_quotes += 1;
                } else if self.html_quotes > 0 {
                    self.quote_depth = self.quote_depth.saturating_sub(1);
                    self.html_quotes -= 1;
                }
            }
            "pre" => {
                self.flush();
                if !closing {
                    if self.code.is_none() {
                        self.code = Some(String::new());
                        self.html_pre = true;
                    }
                } else if self.html_pre {
                    self.html_pre = false;
                    if let Some(code) = self.code.take() {
                        // HTML ignores a newline right after <pre>.
                        self.code_block(code.strip_prefix('\n').unwrap_or(&code));
                    }
                }
            }
            "ul" | "ol" => {
                self.flush();
                if !closing {
                    let start = (name == "ol").then(|| {
                        html::attr(attrs, "start")
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .unwrap_or(1)
                    });
                    self.start_list(start);
                    self.html_lists += 1;
                } else if self.html_lists > 0 {
                    self.lists.pop();
                    self.html_lists -= 1;
                }
            }
            "li" => {
                self.flush();
                match self.lists.last_mut() {
                    Some(ctx) if self.html_lists > 0 => {
                        ctx.item_open = !closing;
                        ctx.first_para = !closing;
                        ctx.task = None;
                    }
                    // A stray <li> outside any list: keep it readable.
                    _ if !closing => self.text("\u{2022} "),
                    _ => {}
                }
            }
            "table" | "tr" | "td" | "th" => self.html_table_tag(name, closing),
            "img" if !closing => {
                if let Some(src) = html::attr(attrs, "src").filter(|s| !s.trim().is_empty()) {
                    let alt = html::attr(attrs, "alt").unwrap_or_default();
                    let width = html::attr(attrs, "width").and_then(|w| parse_img_width(&w));
                    let title = html::attr(attrs, "title").unwrap_or_default();
                    self.image(ImageCtx {
                        dest: src,
                        alt,
                        title,
                        width,
                    })?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A block-level HTML element opens or closes: end the current paragraph and
    /// track `align=`/`style="text-align:"`/`<center>` for the paragraphs inside.
    fn block_boundary(&mut self, name: &str, closing: bool, attrs: &str) {
        self.flush();
        if closing {
            // Pop the nearest matching open element (tolerates missing close tags).
            if let Some(i) = self.align_stack.iter().rposition(|(n, _)| n == name) {
                self.align_stack.truncate(i);
            }
        } else if let Some(a) = html_align(name, attrs) {
            self.align_stack.push((name.to_string(), a));
        }
    }

    /// An element with an `id` (or `<a name>`) is where `[x](#id)` and `<a href="#id">` land.
    fn html_anchor_target(&mut self, name: &str, attrs: &str) {
        let id = html::attr(attrs, "id").or_else(|| if name == "a" { html::attr(attrs, "name") } else { None });
        let Some(id) = id else { return };
        if let Some(&n) = self.anchors.get(&id) {
            if n >= HTML_ANCHOR_BASE && self.emitted_anchors.insert(n) {
                self.push(Child::Bookmark(n));
            }
        }
    }

    fn html_anchor(&mut self, closing: bool, attrs: &str) {
        if closing {
            if self.html_link {
                self.close_html_link();
            }
            return;
        }
        // `<a href="#target">` is an internal jump: harmless, so allowed even with HTML links off.
        if self.link.is_none() {
            if let Some(n) = html::attr(attrs, "href")
                .and_then(|h| h.strip_prefix('#').map(str::to_string))
                .and_then(|f| self.heading_for(&f))
            {
                self.link = Some(LinkCtx {
                    url: String::new(),
                    children: Vec::new(),
                    bookmark: Some(n),
                });
                self.html_link = true;
                return;
            }
        }
        if !self.cfg.html_links || self.link.is_some() {
            // links off, or already inside a link: keep the text only
            if let Some(h) = html::attr(attrs, "href") {
                let why = if self.link.is_some() {
                    "it is inside another link"
                } else {
                    "HTML links are turned off"
                };
                self.note(
                    Code::LinkHtmlDropped,
                    format!("a raw HTML link to `{}` was not made a hyperlink: {why}", short(&h)),
                );
            }
            return;
        }
        let href = html::attr(attrs, "href");
        let url = href
            .as_deref()
            .and_then(|h| safe_link(h, &self.cfg.allowed_link_schemes));
        if url.is_none() {
            if let Some(h) = &href {
                self.note(
                    Code::LinkHtmlDropped,
                    format!("a raw HTML link to `{}` is not an allowed destination", short(h)),
                );
            }
        }
        if let Some(url) = url {
            self.link = Some(LinkCtx {
                url,
                children: Vec::new(),
                bookmark: None,
            });
            self.html_link = true;
        }
    }

    fn close_html_link(&mut self) {
        self.html_link = false;
        if let Some(link) = self.link.take() {
            let target = match link.bookmark {
                Some(n) => LinkTarget::Heading(n),
                None => LinkTarget::External(link.url),
            };
            self.finish_link(target, link.children);
        }
    }

    fn html_table_tag(&mut self, name: &str, closing: bool) {
        if name == "table" {
            if !closing {
                if self.table.is_none() {
                    self.flush();
                    self.table = Some(TableState {
                        html: true,
                        ..TableState::default()
                    });
                }
            } else if self.table.as_ref().is_some_and(|t| t.html) {
                self.close_html_cell();
                self.close_html_row();
                self.finish_table();
            }
            return;
        }
        // Markdown tables (and no table at all) ignore stray row/cell tags.
        if !self.table.as_ref().is_some_and(|t| t.html) {
            return;
        }
        match (name, closing) {
            ("tr", _) => {
                self.close_html_cell();
                self.close_html_row();
            }
            ("td" | "th", false) => {
                self.close_html_cell();
                if let Some(t) = self.table.as_mut() {
                    t.in_head = name == "th";
                }
                self.cell_start();
            }
            ("td" | "th", true) => self.close_html_cell(),
            _ => {}
        }
    }

    fn close_html_cell(&mut self) {
        if self.table.as_ref().is_some_and(|t| t.html && t.cell.is_some()) {
            self.cell_end();
            if let Some(t) = self.table.as_mut() {
                t.in_head = false;
            }
        }
    }

    fn close_html_row(&mut self) {
        if let Some(t) = self.table.as_mut() {
            if t.html && !t.row_cells.is_empty() {
                t.max_cols = t.max_cols.max(t.row_cells.len());
                self.row_end();
            }
        }
    }

    fn finish_table(&mut self) {
        let Some(t) = self.table.take() else { return };
        // A Markdown table that started inside a cell of an HTML table: that table resumes.
        let mut outer = if t.html { None } else { self.outer_tables.pop() };
        if !t.rows.is_empty() {
            // (Word rejects a table without rows, so an empty one is dropped.)
            let cols = t.aligns.len().max(t.max_cols).max(1);
            self.stats.tables += 1;
            let table = Table::new(t.rows)
                .set_grid(column_widths(self.geo.content_w, cols, &t.col_chars))
                .width(self.geo.content_w, WidthType::Dxa);
            match outer.as_mut() {
                Some(o) if o.cell.is_some() => {
                    if let Some(cell) = o.cell.take() {
                        // A cell must end with a paragraph, so one follows the nested table.
                        o.cell = Some(cell.add_table(table).add_paragraph(Paragraph::new()));
                        o.cell_paras += 1;
                    }
                }
                // No open cell: it goes in front of the HTML table, as browsers do.
                _ => self.blocks.push(Block::Table(Box::new(table))),
            }
        }
        self.table = outer;
    }

    /// Close anything raw HTML left open at the end of the document.
    fn finish_html(&mut self) {
        if self.table.as_ref().is_some_and(|t| t.html) {
            self.close_html_cell();
            self.close_html_row();
            self.finish_table();
        }
        if self.html_link {
            self.close_html_link();
        }
        self.flush();
    }

    // ---- footnotes

    /// The parser only makes a footnote reference of `[^label]` when `label` has a definition, so
    /// an undefined one arrives as plain text; report it so it is not mistaken for a footnote.
    fn undefined_footnote_marks(&mut self, text: &str) {
        let mut rest = text;
        while let Some(open) = rest.find("[^") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(']') else { break };
            let label = &after[..close];
            if !label.is_empty() && !label.contains(char::is_whitespace) && !label.contains('[') {
                // Exact bytes when the event is verbatim source (no escapes or entities in it).
                let at = if self.at.len() == text.len() {
                    let start = self.at.start + (text.len() - rest.len()) + open;
                    start..start + close + 3
                } else {
                    self.at.clone()
                };
                self.diags.push(
                    Code::FootnoteUndefined,
                    format!("the footnote reference [^{label}] has no definition and was left as text"),
                    Loc::Rewritten(at),
                );
            }
            rest = &after[close + 1..];
        }
    }

    fn footnote_reference(&mut self, label: &str) -> Result<()> {
        let key = label.to_lowercase();
        let events = match self.footnotes.get(&key) {
            Some(def) if !self.in_footnote => def.events.clone(),
            _ => {
                // Unknown label, or a reference inside a footnote: keep it as written.
                if self.in_footnote {
                    self.note(
                        Code::FootnoteNested,
                        format!("the reference [^{label}] inside a footnote was left as text"),
                    );
                } else {
                    self.note(
                        Code::FootnoteUndefined,
                        format!("the footnote reference [^{label}] has no definition"),
                    );
                }
                self.text(&format!("[^{label}]"));
                return Ok(());
            }
        };
        self.referenced.insert(key);
        // A short definition referenced thousands of times must not balloon the file.
        let size: usize = events
            .iter()
            .map(|(e, _)| match e {
                Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => t.len(),
                _ => 8,
            })
            .sum();
        self.footnote_bytes = self.footnote_bytes.saturating_add(size);
        if self.footnote_bytes > self.cfg.limits.max_input_bytes {
            return Err(CrateError::LimitExceeded(format!(
                "footnotes expand to more than {} bytes",
                self.cfg.limits.max_input_bytes
            )));
        }
        let mut sub = Builder::new(self.pre, self.cfg, self.nonce.clone());
        sub.in_footnote = true;
        sub.anchors = Arc::clone(&self.anchors);
        // One budget for the whole document: the footnote's pictures, equations and element
        // count are charged to the same pipeline and slot list as the body's.
        sub.elements = self.elements;
        sub.images = std::mem::take(&mut self.images);
        sub.resolved = std::mem::take(&mut self.resolved);
        sub.omml = std::mem::take(&mut self.omml);
        let fed = (|| -> Result<()> {
            for (ev, range) in events {
                sub.at = range;
                sub.feed(ev)?;
            }
            sub.flush();
            Ok(())
        })();
        self.elements = sub.elements;
        self.images = std::mem::take(&mut sub.images);
        self.resolved = std::mem::take(&mut sub.resolved);
        self.omml = std::mem::take(&mut sub.omml);
        fed?;
        if let Some(e) = sub.budget_error.take() {
            return Err(e);
        }
        self.stats.pictures += sub.stats.pictures;
        self.stats.tables += sub.stats.tables;
        self.stats.equations += sub.stats.equations;
        self.stats.hyperlinks += sub.stats.hyperlinks;
        self.stats.code_blocks += sub.stats.code_blocks;
        self.diags.absorb(std::mem::take(&mut sub.diags));
        let mut note = Footnote::new();
        let mut any = false;
        for block in sub.blocks {
            match block {
                Block::Para(p) => {
                    note.add_content(*p);
                    any = true;
                }
                Block::Table(t) => {
                    note.add_table(*t);
                    any = true;
                }
            }
        }
        if !any {
            note.add_content(Paragraph::new());
        }
        self.push(Child::Run(Run::new().add_footnote_reference(note)));
        self.stats.footnotes += 1;
        Ok(())
    }

    // ---- the event loop

    /// Handle one event, then finish an image if this event closed one.
    fn feed(&mut self, ev: Event<'_>) -> Result<()> {
        if let Some(e) = self.budget_error.take() {
            return Err(e);
        }
        let image_ended = matches!(ev, Event::End(TagEnd::Image));
        self.event(ev)?;
        if image_ended {
            if self.image_nesting > 0 {
                self.image_nesting -= 1;
            } else if let Some(ctx) = self.image.take() {
                self.image(ctx)?;
            }
        }
        match self.budget_error.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn event(&mut self, ev: Event<'_>) -> Result<()> {
        if let Some(name) = self.hidden {
            match &ev {
                Event::InlineHtml(h) if html::closes_hidden(h, name) => {
                    self.hidden = None;
                    return Ok(());
                }
                Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak | Event::InlineHtml(_) => {
                    return Ok(());
                }
                // A paragraph/cell/item ends the hiding, whether or not the tag closed.
                Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell | TagEnd::Item) => {
                    self.hidden = None;
                }
                _ => {}
            }
        }
        match ev {
            Event::Start(tag) => self.start(tag)?,
            Event::End(tag) => self.end(tag)?,
            Event::Text(t) => {
                if let Some(code) = self.code.as_mut() {
                    code.push_str(&t);
                } else if let Some(img) = self.image.as_mut() {
                    img.alt.push_str(&t);
                } else {
                    self.undefined_footnote_marks(&t);
                    self.prose(&t);
                }
            }
            Event::Code(t) => {
                if let Some(img) = self.image.as_mut() {
                    img.alt.push_str(&t);
                } else {
                    self.count_cell_chars(t.chars().count());
                    let run = self.format(
                        Run::new()
                            .style("InlineCode")
                            .shading(Shading::new().fill(self.cfg.document.code_background.as_str()))
                            .add_text(sanitize_xml_chars(&t)),
                    );
                    self.push(Child::Run(run));
                }
            }
            Event::SoftBreak if self.image.is_none() => self.text(" "),
            Event::HardBreak if self.image.is_none() => {
                self.push(Child::Run(Run::new().add_break(BreakType::TextWrapping)));
            }
            Event::InlineHtml(h) => {
                self.hidden = html::unclosed_hidden(&h);
                if let Some(name) = self.hidden {
                    self.note(
                        Code::HtmlUnsupported,
                        format!("the content of a raw HTML <{name}> element is never shown and was dropped"),
                    );
                }
                self.html_fragment(&h, false)?;
            }
            Event::Html(h) => match self.html.as_mut() {
                Some(buf) => buf.push_str(&h),
                None => self.html_fragment(&h, true)?,
            },
            Event::FootnoteReference(label) => self.footnote_reference(&label)?,
            Event::TaskListMarker(checked) => {
                if let Some(ctx) = self.lists.last_mut() {
                    ctx.task = Some(checked);
                }
            }
            Event::Rule => {
                self.flush();
                self.rule();
            }
            // Math has been rewritten to images by Stage 1; nothing else carries content.
            _ => {}
        }
        Ok(())
    }

    fn start(&mut self, tag: Tag<'_>) -> Result<()> {
        match tag {
            Tag::Paragraph => {
                self.flush();
                self.start_para(ParaKind::Normal);
            }
            Tag::Heading { level, .. } => {
                self.flush();
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.start_para(ParaKind::Heading(n));
                if !self.in_footnote {
                    self.stats.headings += 1;
                }
                self.cur_heading = None;
                if !self.in_footnote {
                    self.cur_heading = Some(self.next_heading);
                    self.next_heading += 1;
                }
            }
            Tag::BlockQuote(kind) => {
                self.flush();
                self.quote_depth += 1;
                // GitHub-style alert (`> [!NOTE]`): a bold label line first.
                if let Some(kind) = kind {
                    let label = match kind {
                        BlockQuoteKind::Note => "Note",
                        BlockQuoteKind::Tip => "Tip",
                        BlockQuoteKind::Important => "Important",
                        BlockQuoteKind::Warning => "Warning",
                        BlockQuoteKind::Caution => "Caution",
                    };
                    self.start_para(ParaKind::Normal);
                    self.bold += 1;
                    self.text(label);
                    self.bold -= 1;
                    self.flush();
                }
            }
            Tag::HtmlBlock => {
                self.flush();
                self.html = Some(String::new());
            }
            Tag::DefinitionList => self.flush(),
            Tag::DefinitionListTitle => {
                self.flush();
                self.start_para(ParaKind::Normal);
                self.bold += 1;
            }
            Tag::DefinitionListDefinition => {
                self.flush();
                self.def_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.code = Some(String::new());
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(info) => highlight::fence_language(&info),
                    CodeBlockKind::Indented => None,
                };
            }
            Tag::List(first) => {
                self.flush();
                self.start_list(first);
            }
            Tag::Item => {
                self.flush();
                if let Some(ctx) = self.lists.last_mut() {
                    ctx.item_open = true;
                    ctx.first_para = true;
                    ctx.task = None;
                }
            }
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Superscript => self.sup += 1,
            Tag::Subscript => self.sub += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link {
                link_type, dest_url, ..
            } => {
                // `<me@example.com>` arrives as a bare address; it is a mailto link.
                let url = if link_type == LinkType::Email && !dest_url.starts_with("mailto:") {
                    format!("mailto:{dest_url}")
                } else {
                    dest_url.to_string()
                };
                self.link = Some(LinkCtx {
                    url,
                    children: Vec::new(),
                    bookmark: None,
                });
            }
            // An image inside another image's alt text only contributes its alt text.
            Tag::Image { .. } if self.image.is_some() => self.image_nesting += 1,
            Tag::Image { dest_url, title, .. } => {
                self.image = Some(ImageCtx {
                    dest: dest_url.to_string(),
                    alt: String::new(),
                    title: title.to_string(),
                    width: None,
                });
            }
            Tag::Table(aligns) => {
                self.flush();
                // A Markdown table inside a cell of an HTML table: keep the outer one.
                if let Some(outer) = self.table.take() {
                    if self.outer_tables.len() < MAX_NESTED_TABLES {
                        self.outer_tables.push(outer);
                    } else {
                        self.note(
                            Code::TableDropped,
                            format!("tables nested deeper than {MAX_NESTED_TABLES} levels lose their outer table"),
                        );
                    }
                }
                self.table = Some(TableState {
                    aligns,
                    ..TableState::default()
                });
            }
            Tag::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    t.in_head = true;
                }
            }
            Tag::TableRow => {}
            Tag::TableCell => self.cell_start(),
            _ => {}
        }
        Ok(())
    }

    fn end(&mut self, tag: TagEnd) -> Result<()> {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.flush(),
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::HtmlBlock => {
                if let Some(raw) = self.html.take() {
                    self.html_fragment(&raw, true)?;
                }
            }
            TagEnd::DefinitionList => self.flush(),
            TagEnd::DefinitionListTitle => {
                self.flush();
                self.bold = self.bold.saturating_sub(1);
            }
            TagEnd::DefinitionListDefinition => {
                self.flush();
                self.def_depth = self.def_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.code_block(&code);
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                if let Some(ctx) = self.lists.last_mut() {
                    ctx.item_open = false;
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Superscript => self.sup = self.sup.saturating_sub(1),
            TagEnd::Subscript => self.sub = self.sub.saturating_sub(1),
            TagEnd::Link => {
                if let Some(link) = self.link.take() {
                    // `[x](#some-heading)` jumps to that heading.
                    let heading = link.url.strip_prefix('#').and_then(|f| self.heading_for(f));
                    if let Some(n) = heading {
                        self.finish_link(LinkTarget::Heading(n), link.children);
                        return Ok(());
                    }
                    match safe_link(&link.url, &self.cfg.allowed_link_schemes) {
                        Some(url) => self.finish_link(LinkTarget::External(url), link.children),
                        // Not an allowed destination: keep the visible text, drop the target.
                        None => {
                            if link.url.starts_with('#') {
                                self.note(
                                    Code::LinkAnchorMissing,
                                    format!("no heading has the anchor `{}`", short(&link.url)),
                                );
                            } else {
                                self.note(
                                    Code::LinkDropped,
                                    format!(
                                        "the link target `{}` is not an allowed destination; only its text was kept",
                                        short(&link.url)
                                    ),
                                );
                            }
                            for c in link.children {
                                self.push(c);
                            }
                        }
                    }
                }
            }
            TagEnd::Image => {
                // Finished by `feed`, which can fail.
            }
            TagEnd::Table => self.finish_table(),
            TagEnd::TableHead => {
                self.row_end();
                if let Some(t) = self.table.as_mut() {
                    t.in_head = false;
                }
            }
            TagEnd::TableRow => self.row_end(),
            TagEnd::TableCell => self.cell_end(),
            _ => {}
        }
        Ok(())
    }

    /// The number of the heading that the anchor `fragment` (percent-encoded) names.
    fn heading_for(&self, fragment: &str) -> Option<usize> {
        let decoded = percent_encoding::percent_decode_str(fragment).decode_utf8_lossy();
        self.anchors
            .get(decoded.as_ref())
            .or_else(|| self.anchors.get(&decoded.to_lowercase()))
            .copied()
    }

    fn finish_link(&mut self, target: LinkTarget, children: Vec<Child>) {
        let mut h = match target {
            LinkTarget::External(url) => Hyperlink::new(url, HyperlinkType::External),
            LinkTarget::Heading(n) => Hyperlink::new(bookmark_name(n), HyperlinkType::Anchor),
        };
        let mut loose = Vec::new();
        for c in children {
            match c {
                Child::Run(r) | Child::Picture { run: r, .. } => {
                    h = h.add_run(r.style("Hyperlink"));
                }
                Child::Omml { slot, .. } => {
                    h = h.add_run(Run::new().add_text(marker(&self.nonce, slot)));
                }
                other @ (Child::Link(_) | Child::Bookmark(_)) => loose.push(other),
            }
        }
        self.push(Child::Link(h));
        self.stats.hyperlinks += 1;
        for c in loose {
            self.push(c);
        }
    }
}

/// Alignment requested by an HTML element: `<center>`, `align="..."`, or
/// `style="text-align:..."`.
fn html_align(name: &str, attrs: &str) -> Option<AlignmentType> {
    if name == "center" {
        return Some(AlignmentType::Center);
    }
    let value = html::attr(attrs, "align").or_else(|| {
        html::attr(attrs, "style").and_then(|style| {
            style.split(';').find_map(|decl| {
                let (k, v) = decl.split_once(':')?;
                (k.trim().eq_ignore_ascii_case("text-align")).then(|| v.trim().to_string())
            })
        })
    })?;
    match value.trim().to_ascii_lowercase().as_str() {
        "center" | "middle" => Some(AlignmentType::Center),
        "right" | "end" => Some(AlignmentType::Right),
        "left" | "start" => Some(AlignmentType::Left),
        "justify" => Some(AlignmentType::Both),
        _ => None,
    }
}

/// `width="120"`, `"120px"` or `"50%"`; anything else is ignored.
fn parse_img_width(raw: &str) -> Option<ImgWidth> {
    let raw = raw.trim().to_ascii_lowercase();
    if let Some(pct) = raw.strip_suffix('%') {
        return pct
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|p| (1..=100).contains(p))
            .map(ImgWidth::Percent);
    }
    let px = raw.strip_suffix("px").unwrap_or(&raw).trim();
    px.parse::<u64>()
        .ok()
        .filter(|p| (1..=20_000).contains(p))
        .map(ImgWidth::Px)
}

// ----------------------------------------------------------- OMML injection

/// Position of the last `<tag` (exactly `<w:p>`/`<w:p ` and not `<w:pPr>`)
/// before `before`.
fn rfind_open(doc: &str, tag: &str, before: usize) -> Option<usize> {
    let needle = format!("<{tag}");
    let mut end = before;
    while let Some(pos) = doc[..end].rfind(&needle) {
        match doc.as_bytes().get(pos + needle.len()) {
            Some(b'>') | Some(b' ') => return Some(pos),
            _ => end = pos,
        }
    }
    None
}

/// Replace marker runs/paragraphs in `document.xml` with native OMML.
///
/// Each marker is located with a forward-moving cursor (markers occur in slot
/// order) and the output is assembled in one pass, so the cost is linear in
/// the document size however many equations it holds.
fn inject_omml(doc: &str, nonce: &str, slots: &[OmmlSlot], footnotes: bool) -> Result<String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::with_capacity(slots.len());
    let mut cursor = 0;
    for (idx, slot) in slots.iter().enumerate().filter(|(_, s)| s.footnote == footnotes) {
        let m = marker(nonce, idx);
        let lost = || CrateError::DocxGenerationError(format!("equation placeholder {idx} missing from document.xml"));
        let at = match doc[cursor..].find(&m) {
            Some(rel) => cursor + rel,
            None => doc.find(&m).ok_or_else(lost)?, // out of order: fall back to a full search
        };
        let (open_tag, close_tag) = if slot.block {
            ("w:p", "</w:p>")
        } else {
            ("w:r", "</w:r>")
        };
        let start = rfind_open(doc, open_tag, at).ok_or_else(lost)?;
        let end = doc[at..].find(close_tag).ok_or_else(lost)? + at + close_tag.len();
        let replacement = if slot.block {
            // Keep the paragraph's own properties: list numbering, quote or custom-block indent.
            let para = &doc[start..end];
            let props = para
                .find("<w:pPr>")
                .and_then(|a| para[a..].find("</w:pPr>").map(|b| &para[a..a + b + "</w:pPr>".len()]))
                .unwrap_or("");
            format!(
                "<w:p>{props}<m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara></w:p>",
                slot.xml
            )
        } else {
            format!("<m:oMath>{}</m:oMath>", slot.xml)
        };
        cursor = end;
        edits.push((start, end, replacement));
    }
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(doc.len());
    let mut last = 0;
    for (start, end, replacement) in &edits {
        if *start < last {
            return Err(CrateError::DocxGenerationError(
                "overlapping equation placeholders".into(),
            ));
        }
        out.push_str(&doc[last..*start]);
        out.push_str(replacement);
        last = *end;
    }
    out.push_str(&doc[last..]);
    // Declare the math namespace on the root, as Word does.
    crate::omml::ensure_math_namespace_in(&out, if footnotes { "<w:footnotes" } else { "<w:document" })
        .map_err(|e| CrateError::DocxGenerationError(e.to_string()))
}

// ------------------------------------------------------------------ driver

/// Footnote bodies by label (the first definition of a label wins).
fn collect_footnotes(markdown: &str, sup_sub: bool) -> HashMap<String, FootnoteDef<'_>> {
    let mut map = HashMap::new();
    let mut open: Option<OpenDefinition<'_>> = None;
    for (ev, range) in merged_events(markdown, sup_sub) {
        match ev {
            Event::Start(Tag::FootnoteDefinition(label)) => {
                open = Some(OpenDefinition {
                    label: label.to_lowercase(),
                    def: FootnoteDef {
                        events: Vec::new(),
                        range,
                    },
                });
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some(OpenDefinition { label, def }) = open.take() {
                    map.entry(label).or_insert(def);
                }
            }
            other => {
                if let Some(o) = open.as_mut() {
                    o.def.events.push((other, range));
                }
            }
        }
    }
    map
}

/// The document parts and what was found while building them.
pub(crate) struct Built {
    pub parts: crate::docx::XMLDocx,
    pub report: Report,
}

/// Convert, then check the finished document, then write it. `original` is the text the caller
/// passed in; findings point into it.
pub fn build_docx<W: std::io::Write + std::io::Seek>(
    pre: &Preprocessed,
    cfg: &Config,
    original: &str,
    writer: W,
) -> Result<Report> {
    let Built { parts, report } = build_parts(pre, cfg, original)?;
    if cfg.loss_policy == crate::LossPolicy::Reject && report.has_warnings() {
        return Err(CrateError::LossyContent {
            diagnostics: report.diagnostics,
        });
    }
    pre.ctl.check()?;
    crate::pack::pack(parts, cfg.compression_level, cfg.limits.max_output_bytes, writer)?;
    Ok(report)
}

/// Build every part of the document except the archive.
pub(crate) fn build_parts(pre: &Preprocessed, cfg: &Config, original: &str) -> Result<Built> {
    // Identifier counters restart for every conversion, so equal input gives equal ids and one
    // conversion cannot observe another's progress.
    let _ids = crate::docx::IdScope::new();
    let nonce = crate::renderer::asset_stem("", &[pre.markdown.as_bytes()]).replace('_', "");
    let mut b = Builder::new(pre, cfg, nonce);
    b.footnotes = collect_footnotes(&pre.markdown, cfg.superscript_subscript);
    let (anchors, headings) = heading_anchors(&pre.markdown, cfg.superscript_subscript);
    b.anchors = Arc::new(anchors);

    let mut in_definition = false;
    let mut events = 0usize;
    for (event, range) in merged_events(&pre.markdown, cfg.superscript_subscript) {
        // Footnote bodies are emitted where they are referenced, not in place.
        match &event {
            Event::Start(Tag::FootnoteDefinition(_)) => in_definition = true,
            Event::End(TagEnd::FootnoteDefinition) => {
                in_definition = false;
                continue;
            }
            _ => {}
        }
        if !in_definition {
            events += 1;
            if events.is_multiple_of(256) {
                pre.ctl.check()?;
            }
            b.at = range;
            b.feed(event)?;
        }
    }
    b.finish_html();
    b.report_unreferenced_footnotes();
    if let Some(toc) = &cfg.document.table_of_contents {
        let blocks = b.contents_blocks(toc, &headings)?;
        b.blocks.splice(0..0, blocks);
    }

    let (bullets, ordered) = numbering_definitions();
    let now = now_rfc3339();
    let mut docx = base_docx(&cfg.document, &b.geo)
        .created_at(&now)
        .updated_at(&now)
        .add_abstract_numbering(bullets)
        .add_abstract_numbering(ordered)
        .add_numbering(Numbering::new(BULLET_NUM_ID, BULLET_ABSTRACT_ID));
    if cfg.document.number_headings {
        docx = docx
            .add_abstract_numbering(heading_numbering())
            .add_numbering(Numbering::new(HEADING_NUM_ID, HEADING_ABSTRACT_ID));
    }
    for n in b.ordered_numberings.drain(..) {
        docx = docx.add_numbering(n);
    }
    for block in b.blocks.drain(..) {
        docx = match block {
            Block::Para(p) => docx.add_paragraph(*p),
            Block::Table(t) => docx.add_table(*t),
        };
    }

    if cfg.document.page_numbers {
        docx = docx.footer(
            Footer::new().add_paragraph(
                Paragraph::new()
                    .align(AlignmentType::Center)
                    .add_page_num(PageNum::new()),
            ),
        );
    }

    let mut parts = docx.build();
    parts.footnotes = add_footnote_marks(std::mem::take(&mut parts.footnotes));
    let (front_title, front_author) = pre.meta.clone();
    let title = cfg.document.title.clone().or(front_title);
    let author = cfg.document.author.clone().or(front_author);
    if title.is_some() || author.is_some() {
        if let Ok(mut core) = String::from_utf8(std::mem::take(&mut parts.doc_props.core)) {
            if let Some(a) = &author {
                let a = xml_text(a);
                core = core
                    .replace(
                        "<dc:creator>unknown</dc:creator>",
                        &format!("<dc:creator>{a}</dc:creator>"),
                    )
                    .replace(
                        "<cp:lastModifiedBy>unknown</cp:lastModifiedBy>",
                        &format!("<cp:lastModifiedBy>{a}</cp:lastModifiedBy>"),
                    );
            }
            if let Some(t) = &title {
                core = core.replace(
                    "<dc:creator>",
                    &format!("<dc:title>{}</dc:title><dc:creator>", xml_text(t)),
                );
            }
            parts.doc_props.core = core.into_bytes();
        }
    }
    if !b.omml.is_empty() {
        debug_assert!(cfg.math.mode == MathMode::NativeOmml);
        let xml = String::from_utf8(std::mem::take(&mut parts.document))
            .map_err(|e| CrateError::DocxGenerationError(format!("document.xml is not UTF-8: {e}")))?;
        parts.document = inject_omml(&xml, &b.nonce, &b.omml, false)?.into_bytes();
        if b.omml.iter().any(|s| s.footnote) {
            let notes = String::from_utf8(std::mem::take(&mut parts.footnotes))
                .map_err(|e| CrateError::DocxGenerationError(format!("footnotes.xml is not UTF-8: {e}")))?;
            parts.footnotes = inject_omml(&notes, &b.nonce, &b.omml, true)?.into_bytes();
        }
    }

    let in_memory = crate::pack::uncompressed_size(&parts);
    if in_memory > cfg.limits.max_uncompressed_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "the document parts total {in_memory} bytes (Limits::max_uncompressed_bytes {})",
            cfg.limits.max_uncompressed_bytes
        )));
    }

    let mut collected = pre.diags.clone();
    collected.absorb(std::mem::take(&mut b.diags));
    Ok(Built {
        parts,
        report: Report {
            diagnostics: crate::diagnostics::finish(&collected, &pre.offsets, original),
            stats: b.stats,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut png = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        png.extend_from_slice(&w.to_be_bytes());
        png.extend_from_slice(&h.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        png
    }

    #[test]
    fn bare_urls_are_split_out_like_github_does() {
        fn show(t: &str) -> Vec<String> {
            split_bare_urls(t)
                .into_iter()
                .map(|p| match p {
                    UrlPiece::Text(s) => format!("t:{s}"),
                    UrlPiece::Url(s) => format!("u:{s}"),
                })
                .collect()
        }
        assert_eq!(
            show("see https://example.com/a?b=1."),
            ["t:see ", "u:https://example.com/a?b=1", "t:."]
        );
        assert_eq!(show("(http://x.org/a_(b))"), ["t:(", "u:http://x.org/a_(b)", "t:)"]);
        assert_eq!(
            show("go to www.example.com, now"),
            ["t:go to ", "u:www.example.com", "t:, now"]
        );
        assert_eq!(
            show("fooshttps://x.org no"),
            ["t:fooshttps://x.org no"],
            "needs a word boundary"
        );
        assert_eq!(show("www. alone and http:// alone"), ["t:www. alone and http:// alone"]);
        assert_eq!(
            show("日本語 https://例え.jp/ok 終"),
            ["t:日本語 ", "u:https://例え.jp/ok", "t: 終"]
        );
        assert_eq!(show(""), Vec::<String>::new());
    }

    #[test]
    fn column_widths_follow_content_and_always_fill_the_table() {
        let w = column_widths(9000, 3, &[2, 40, 10]);
        assert_eq!(w.iter().sum::<usize>(), 9000);
        assert!(w[1] > w[2] && w[2] > w[0], "{w:?}");
        // A verbose column cannot starve the others.
        let w = column_widths(9000, 2, &[1000, 1]);
        assert!(w[1] >= 9000 * 6 / 54 - 1, "{w:?}");
        // No data (an empty table, extra columns): equal-ish shares, never a zero or a panic.
        let w = column_widths(9000, 4, &[]);
        assert_eq!(w, vec![2250; 4]);
        assert_eq!(column_widths(5, 3, &[]).iter().sum::<usize>(), 5);
        assert!(column_widths(0, 0, &[]).is_empty());
    }

    #[test]
    fn indents_never_overflow_or_exceed_the_widest_page() {
        for n in [0, 1, 1440, 31_680, 31_681, usize::MAX / 2, usize::MAX] {
            let v = indent_twips(n);
            assert!((0..=31_680).contains(&v), "{n} -> {v}");
        }
        assert_eq!(indent_twips(720), 720);
        assert_eq!(indent_twips(360usize.saturating_mul(6_000_000)), 31_680);
    }

    #[test]
    fn hostile_dimensions_never_produce_a_picture() {
        let limits = crate::Limits::trusted();
        for (w, h) in [
            (4_000_000_000, 4_000_000_000),
            (u32::MAX, 1),
            (1, u32::MAX),
            (0, 0),
            (20_000, 20_000),
        ] {
            let r =
                ImagePipeline::new().normalize(&limits, &crate::control::Ctl::none(), &png_header(w, h), "hostile.png");
            assert!(!matches!(r, Ok(Outcome::Picture(_))), "{w}x{h} must not be embedded");
        }
        // Garbage and unsupported formats are "not an image", not a crash.
        for junk in [&b"<svg xmlns='x'/>"[..], &[][..], &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0][..]] {
            let r = ImagePipeline::new().normalize(&limits, &crate::control::Ctl::none(), junk, "junk");
            assert!(matches!(r, Ok(Outcome::Unsupported(_))), "{junk:?}");
        }
    }

    #[test]
    fn display_size_is_bounded_and_keeps_aspect_ratio() {
        // 6in x 3in at 288 dpi, well inside the page: exact.
        assert_eq!(
            display_size_emu(&Geometry::default(), 1728, 864, 288),
            Some((5_486_400, 2_743_200))
        );
        // Wider than the page: clamped to the text width, ratio kept.
        let (w, h) = display_size_emu(&Geometry::default(), 10_000, 5_000, 96).unwrap();
        assert_eq!(u128::from(w), Geometry::default().max_img_w);
        assert!((i64::from(w) - 2 * i64::from(h)).abs() <= 1);
        // Very tall and narrow: clamped to the text height.
        let (w, h) = display_size_emu(&Geometry::default(), 100, 50_000, 96).unwrap();
        assert_eq!(u128::from(h), Geometry::default().max_img_h);
        assert!(w >= 1);
        // Extremes never overflow, wrap, or divide by zero.
        assert!(display_size_emu(&Geometry::default(), u32::MAX, u32::MAX, 0).is_some());
        assert!(display_size_emu(&Geometry::default(), 1, 1, u32::MAX).is_some());
        assert_eq!(display_size_emu(&Geometry::default(), 0, 0, 96), Some((1, 1)));
    }
}
