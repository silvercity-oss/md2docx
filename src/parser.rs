//! Stages 2 and 3: stream CommonMark events out of `pulldown-cmark` and map
//! them onto `docx-rs` elements.
//!
//! `docx-rs` cannot express OMML, so native equations are emitted as marker
//! runs and swapped for real `<m:oMath>` XML in the rendered `document.xml`
//! before the package is zipped (see [`inject_omml`]).

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use docx_rs::{
    AbstractNumbering, AlignmentType, BorderType, BreakType, Docx, Footnote, Hyperlink, HyperlinkType, IndentLevel,
    Level, LevelJc, LevelOverride, LevelText, LineSpacing, NumberFormat, Numbering, NumberingId, PageMargin, Paragraph,
    ParagraphBorder, ParagraphBorderPosition, ParagraphBorders, Pic, Run, RunFonts, Shading, SpecialIndentType, Start,
    Style, StyleType, Table, TableCell, TableRow, VertAlignType, WidthType,
};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd, TextMergeStream,
};

use crate::config::{CodeBlockLayout, Config, ImageAccess, MathMode};
use crate::error::{CrateError, Result};
use crate::extend::TagStyle;
use crate::html::{self, Piece};
use crate::omml::ensure_math_namespace;
use crate::preprocess::{Asset, AssetKind, AssetSource, Preprocessed};
use crate::safety::{read_local_image, read_regular_file, safe_link, sanitize_xml_chars};

/// Parser options shared by Stage 1 (with `math`) and Stage 2 (without: by then
/// every formula has been rewritten, and any leftover `$` is literal text).
pub(crate) fn markdown_options(math: bool) -> Options {
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
            .size(19),
    )
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
}

enum Block {
    Para(Box<Paragraph>),
    Table(Box<Table>),
}

struct OmmlSlot {
    xml: String,
    /// Decided when the paragraph is flushed: replace the whole `<w:p>`.
    block: bool,
}

struct Builder<'a> {
    pre: &'a Preprocessed,
    cfg: &'a Config,
    geo: Geometry,
    base_dir: PathBuf,
    nonce: String,
    /// Footnote bodies by label, collected before the main pass.
    footnotes: HashMap<String, Vec<Event<'a>>>,
    /// True while building the content of a footnote (no pictures/OMML there).
    in_footnote: bool,
    /// Bytes of footnote content expanded so far (amplification guard).
    footnote_bytes: usize,
    /// Bytes of images embedded so far.
    image_bytes: u64,
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
    quote_depth: usize,
    lists: Vec<ListCtx>,
    ordered_numberings: Vec<Numbering>,
    next_num_id: usize,
    table: Option<TableState>,
    omml: Vec<OmmlSlot>,
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

fn marker(nonce: &str, slot: usize) -> String {
    format!("SILVERCITYOMML{nonce}X{slot}X")
}

/// Refuse images whose header claims more pixels than this (50 MP; a decoded
/// RGBA frame of that size is 200 MB).
const MAX_IMAGE_PIXELS: u64 = 50_000_000;
/// Allocation ceiling handed to the image decoders.
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

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

/// Build a `Pic` from image bytes. docx-rs only embeds PNG, so other formats
/// are decoded and re-encoded; `Pic::new` is avoided because it panics on
/// undecodable input.
///
/// `Ok(None)` means "not an image we can embed" (SVG, WebP, corrupt data); the
/// caller shows the alt text. An error means the image is dangerous or too big.
/// The picture and the size of the PNG that will be embedded (JPEG and GIF are re-encoded, and
/// may grow a lot).
fn make_picture(
    g: &Geometry,
    bytes: Vec<u8>,
    dpi: u32,
    origin: &Path,
    width: Option<ImgWidth>,
) -> Result<Option<(Pic, usize)>> {
    let reader = || image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format();
    let Ok((w, h)) = reader().map_err(|e| CrateError::io(origin, e))?.into_dimensions() else {
        return Ok(None);
    };
    // Judge the *claimed* size before anything allocates for it.
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        return Err(CrateError::LimitExceeded(format!(
            "image {} is {w}x{h} pixels (limit {MAX_IMAGE_PIXELS})",
            origin.display()
        )));
    }
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let png = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        bytes.clone()
    } else {
        let mut r = reader().map_err(|e| CrateError::io(origin, e))?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_DECODE_ALLOC);
        r.limits(limits);
        let Ok(img) = r.decode() else { return Ok(None) };
        let mut out = Cursor::new(Vec::new());
        if img.write_to(&mut out, image::ImageFormat::Png).is_err() {
            return Ok(None);
        }
        out.into_inner()
    };
    let requested = width.map(|wd| match wd {
        ImgWidth::Px(px) => u128::from(px) * 9525, // 1 CSS px = 9525 EMU
        ImgWidth::Percent(pct) => g.max_img_w * u128::from(pct.min(100)) / 100,
    });
    let sized = match requested {
        Some(width_emu) if width_emu > 0 => display_size_for_width(g, w, h, width_emu),
        _ => display_size_emu(g, w, h, dpi),
    };
    let (we, he) = sized
        .ok_or_else(|| CrateError::LimitExceeded(format!("image {} ({w}x{h} px) is out of range", origin.display())))?;
    let png_len = png.len();
    Ok(Some((Pic::new_with_dimensions(png, w, h).size(we, he), png_len)))
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
            in_footnote: false,
            footnote_bytes: 0,
            image_bytes: 0,
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
            quote_depth: 0,
            lists: Vec::new(),
            ordered_numberings: Vec::new(),
            next_num_id: BULLET_NUM_ID + 1,
            table: None,
            omml: Vec::new(),
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
        let run = self.format(Run::new().add_text(sanitize_xml_chars(t)));
        self.push(Child::Run(run));
    }

    fn start_para(&mut self, kind: ParaKind) {
        self.cur = Some(Vec::new());
        self.kind = kind;
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
        let block_omml = match (solo, children.first()) {
            (true, Some(Child::Omml { slot, display: true })) => Some(*slot),
            _ => None,
        };

        let mut p = Paragraph::new();
        if let ParaKind::Heading(n) = kind {
            p = p.style(&format!("Heading{n}"));
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
        let body = text.strip_suffix('\n').unwrap_or(text);
        let left = match (self.lists.last(), self.quote_depth) {
            (Some(ctx), _) if ctx.item_open => Some(indent_twips(720 * (ctx.level + 1))),
            (_, q) if q > 0 => Some(indent_twips(360usize.saturating_mul(q))),
            _ => None,
        };
        let new_paragraph = || {
            let mut p = Paragraph::new().style("CodeBlock");
            p.property = p
                .property
                .shading(Shading::new().fill(self.cfg.document.code_background.as_str()));
            if let Some(l) = left {
                p = p.indent(Some(l), None, None, None);
            }
            p
        };
        // A raw tab inside `<w:t>` is not reliably shown by Word; use a real tab element.
        let add_line = |mut run: Run, line: &str| {
            let line = sanitize_xml_chars(line.trim_end_matches('\r'));
            for (i, part) in line.split('\t').enumerate() {
                if i > 0 {
                    run = run.add_tab();
                }
                if !part.is_empty() {
                    run = run.add_text(part);
                }
            }
            run
        };
        match self.cfg.document.code_blocks {
            CodeBlockLayout::LinePerParagraph => {
                for line in body.split('\n') {
                    if self.budget_error.is_some() {
                        break;
                    }
                    let p = new_paragraph().add_run(add_line(Run::new(), line));
                    self.emit(p);
                }
            }
            CodeBlockLayout::SingleParagraph => {
                // One paragraph per chunk of lines: LibreOffice's layout time grows with the
                // square of the lines in a paragraph (100k lines took 12 minutes).
                let all: Vec<&str> = body.split('\n').collect();
                for chunk in all.chunks(SINGLE_PARAGRAPH_MAX_LINES) {
                    if self.budget_error.is_some() {
                        break;
                    }
                    let mut run = Run::new();
                    for (i, line) in chunk.iter().enumerate() {
                        if i > 0 {
                            run = run.add_break(BreakType::TextWrapping);
                        }
                        run = add_line(run, line);
                    }
                    self.charge(chunk.len() / 16);
                    let p = new_paragraph().add_run(run);
                    self.emit(p);
                }
            }
        }
    }

    // ---- images and assets

    /// Count `n` more embedded image bytes against the document budget.
    fn spend_image_bytes(&mut self, n: u64) -> Result<()> {
        self.image_bytes = self.image_bytes.saturating_add(n);
        if self.image_bytes > self.cfg.limits.max_total_image_bytes {
            return Err(CrateError::LimitExceeded(format!(
                "embedded images exceed {} bytes in total",
                self.cfg.limits.max_total_image_bytes
            )));
        }
        Ok(())
    }

    /// `[alt text]` stand-in for an image that is not embedded.
    fn placeholder(&mut self, alt: &str, fallback: &str) {
        let label = if alt.trim().is_empty() { fallback } else { alt };
        let run = self.format(Run::new().add_text(sanitize_xml_chars(&format!("[{label}]"))).italic());
        self.push(Child::Run(run));
    }

    fn image(&mut self, ctx: ImageCtx) -> Result<()> {
        if let Some(asset) = self.pre.assets.get(&ctx.dest) {
            let asset = asset.clone();
            return self.asset(&asset);
        }
        let dest = ctx.dest.as_str();
        let lower = dest.to_ascii_lowercase();
        // Remote and inline images are never fetched or decoded (no network
        // access by design); keep the alt text so nothing vanishes silently.
        let remote = lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("data:");
        if remote || self.in_footnote || self.cfg.image_access == ImageAccess::None {
            self.placeholder(&ctx.alt, dest);
            return Ok(());
        }
        let (path, bytes) = read_local_image(
            dest,
            &self.base_dir,
            self.cfg.image_access,
            self.cfg.limits.max_image_bytes,
        )?;
        let input_len = bytes.len() as u64;
        self.spend_image_bytes(input_len)?;
        match make_picture(&self.geo, bytes, 96, &path, ctx.width)? {
            Some((pic, png_len)) => {
                // A JPEG or GIF is embedded as a (much larger) PNG: count what is embedded.
                self.spend_image_bytes((png_len as u64).saturating_sub(input_len))?;
                self.push(Child::Picture {
                    run: Run::new().add_image(pic),
                    kind: AssetKind::Graph,
                });
            }
            // A format we cannot embed (SVG, WebP, corrupt data): keep the alt text.
            None => self.placeholder(&ctx.alt, "image"),
        }
        Ok(())
    }

    fn asset(&mut self, asset: &Asset) -> Result<()> {
        if self.in_footnote {
            // Footnote parts cannot carry pictures or OMML here; show the source.
            match asset.kind {
                AssetKind::Graph => self.placeholder("", "diagram"),
                _ => self.text(&asset.original),
            }
            return Ok(());
        }
        match &asset.source {
            AssetSource::Png(path) => {
                let bytes = read_regular_file(path, "rendered asset", self.cfg.limits.max_image_bytes)?;
                let input_len = bytes.len() as u64;
                self.spend_image_bytes(input_len)?;
                let dpi = match asset.kind {
                    AssetKind::Graph => self.cfg.graphs.dpi,
                    // Formulas are typeset at 12pt; body text is 11pt.
                    _ => self.cfg.math.dpi.saturating_mul(12) / 11,
                };
                match make_picture(&self.geo, bytes, dpi, path, None)? {
                    Some((pic, png_len)) => {
                        self.spend_image_bytes((png_len as u64).saturating_sub(input_len))?;
                        self.push(Child::Picture {
                            run: Run::new().add_image(pic),
                            kind: asset.kind,
                        });
                    }
                    None => {
                        return Err(CrateError::ExternalToolFailed {
                            program: "renderer".into(),
                            detail: format!("{} is not a decodable image", path.display()),
                        })
                    }
                }
            }
            AssetSource::Omml(body) => {
                let display = asset.kind == AssetKind::DisplayMath;
                self.omml.push(OmmlSlot {
                    xml: body.clone(),
                    block: false,
                });
                self.push(Child::Omml {
                    slot: self.omml.len() - 1,
                    display,
                });
            }
        }
        Ok(())
    }

    // ---- lists and tables

    fn start_list(&mut self, first: Option<u64>) {
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
        for piece in html::tokenize(raw) {
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
                    self.image(ImageCtx { dest: src, alt, width })?;
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

    fn html_anchor(&mut self, closing: bool, attrs: &str) {
        if closing {
            if self.html_link {
                self.close_html_link();
            }
            return;
        }
        if !self.cfg.html_links || self.link.is_some() {
            return; // links off, or already inside a link: keep the text only
        }
        let url = html::attr(attrs, "href").and_then(|h| safe_link(&h, &self.cfg.allowed_link_schemes));
        if let Some(url) = url {
            self.link = Some(LinkCtx {
                url,
                children: Vec::new(),
            });
            self.html_link = true;
        }
    }

    fn close_html_link(&mut self) {
        self.html_link = false;
        if let Some(link) = self.link.take() {
            self.finish_link(link.url, link.children);
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
            let table = Table::new(t.rows)
                .set_grid(vec![self.geo.content_w / cols; cols])
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

    fn footnote_reference(&mut self, label: &str) -> Result<()> {
        let events = match self.footnotes.get(&label.to_lowercase()) {
            Some(e) if !self.in_footnote => e.clone(),
            _ => {
                // Unknown label, or a reference inside a footnote: keep it as written.
                self.text(&format!("[^{label}]"));
                return Ok(());
            }
        };
        // A short definition referenced thousands of times must not balloon the file.
        let size: usize = events
            .iter()
            .map(|e| match e {
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
        sub.elements = self.elements;
        for ev in events {
            sub.feed(ev)?;
        }
        sub.flush();
        self.elements = sub.elements;
        if let Some(e) = sub.budget_error.take() {
            return Err(e);
        }
        let mut note = Footnote::new();
        let mut any = false;
        for block in sub.blocks {
            if let Block::Para(p) = block {
                note.add_content(*p);
                any = true;
            }
        }
        if !any {
            note.add_content(Paragraph::new());
        }
        self.push(Child::Run(Run::new().add_footnote_reference(note)));
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
                    self.text(&t);
                }
            }
            Event::Code(t) => {
                if let Some(img) = self.image.as_mut() {
                    img.alt.push_str(&t);
                } else {
                    let run = self.format(
                        Run::new()
                            .style("InlineCode")
                            .shading(Shading::new().fill(self.cfg.document.code_background.as_str()))
                            .add_text(sanitize_xml_chars(&t)),
                    );
                    self.push(Child::Run(run));
                }
            }
            Event::SoftBreak => {
                if self.image.is_none() {
                    self.text(" ");
                }
            }
            Event::HardBreak => {
                if self.image.is_none() {
                    self.push(Child::Run(Run::new().add_break(BreakType::TextWrapping)));
                }
            }
            Event::InlineHtml(h) => {
                self.hidden = html::unclosed_hidden(&h);
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
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
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
                });
            }
            // An image inside another image's alt text only contributes its alt text.
            Tag::Image { .. } if self.image.is_some() => self.image_nesting += 1,
            Tag::Image { dest_url, .. } => {
                self.image = Some(ImageCtx {
                    dest: dest_url.to_string(),
                    alt: String::new(),
                    width: None,
                });
            }
            Tag::Table(aligns) => {
                self.flush();
                // A Markdown table inside a cell of an HTML table: keep the outer one.
                if let Some(outer) = self.table.take() {
                    if self.outer_tables.len() < MAX_NESTED_TABLES {
                        self.outer_tables.push(outer);
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
            TagEnd::Link => {
                if let Some(link) = self.link.take() {
                    match safe_link(&link.url, &self.cfg.allowed_link_schemes) {
                        Some(url) => self.finish_link(url, link.children),
                        // Not an allowed destination: keep the visible text, drop the target.
                        None => {
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

    fn finish_link(&mut self, url: String, children: Vec<Child>) {
        if self.in_footnote {
            // footnotes.xml has no relationships part here, so a hyperlink would point at a
            // relationship id that does not exist and make the package invalid. Keep the
            // text, styled as a link.
            for c in children {
                match c {
                    Child::Run(r) | Child::Picture { run: r, .. } => self.push(Child::Run(r.style("Hyperlink"))),
                    other => self.push(other),
                }
            }
            return;
        }
        let mut h = Hyperlink::new(url, HyperlinkType::External);
        let mut loose = Vec::new();
        for c in children {
            match c {
                Child::Run(r) | Child::Picture { run: r, .. } => {
                    h = h.add_run(r.style("Hyperlink"));
                }
                Child::Omml { slot, .. } => {
                    h = h.add_run(Run::new().add_text(marker(&self.nonce, slot)));
                }
                other @ Child::Link(_) => loose.push(other),
            }
        }
        self.push(Child::Link(h));
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
fn inject_omml(doc: &str, nonce: &str, slots: &[OmmlSlot]) -> Result<String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::with_capacity(slots.len());
    let mut cursor = 0;
    for (idx, slot) in slots.iter().enumerate() {
        let m = marker(nonce, idx);
        let lost = || CrateError::DocxGenerationError(format!("equation placeholder {idx} missing from document.xml"));
        let at = match doc[cursor..].find(&m) {
            Some(rel) => cursor + rel,
            None => doc.find(&m).ok_or_else(lost)?, // out of order: fall back to a full search
        };
        let (open_tag, close_tag, replacement) = if slot.block {
            (
                "w:p",
                "</w:p>",
                format!("<w:p><m:oMathPara><m:oMath>{}</m:oMath></m:oMathPara></w:p>", slot.xml),
            )
        } else {
            ("w:r", "</w:r>", format!("<m:oMath>{}</m:oMath>", slot.xml))
        };
        let start = rfind_open(doc, open_tag, at).ok_or_else(lost)?;
        let end = doc[at..].find(close_tag).ok_or_else(lost)? + at + close_tag.len();
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
    ensure_math_namespace(&out).map_err(|e| CrateError::DocxGenerationError(e.to_string()))
}

// ------------------------------------------------------------------ driver

/// Stage 2 + 3: parse the rewritten Markdown and emit the `.docx` bytes.
/// Footnote bodies by label (the first definition of a label wins).
fn collect_footnotes(markdown: &str) -> HashMap<String, Vec<Event<'_>>> {
    let mut map = HashMap::new();
    let mut open: Option<(String, Vec<Event<'_>>)> = None;
    for ev in TextMergeStream::new(Parser::new_ext(markdown, markdown_options(false))) {
        match ev {
            Event::Start(Tag::FootnoteDefinition(label)) => open = Some((label.to_lowercase(), Vec::new())),
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some((label, events)) = open.take() {
                    map.entry(label).or_insert(events);
                }
            }
            other => {
                if let Some((_, events)) = open.as_mut() {
                    events.push(other);
                }
            }
        }
    }
    map
}

pub fn build_docx<W: std::io::Write + std::io::Seek>(pre: &Preprocessed, cfg: &Config, writer: W) -> Result<()> {
    let nonce = crate::renderer::asset_stem("", &[pre.markdown.as_bytes()]).replace('_', "");
    let mut b = Builder::new(pre, cfg, nonce);
    b.footnotes = collect_footnotes(&pre.markdown);

    let mut in_definition = false;
    // Adjacent text events (escapes, entities, stray delimiters) become one run, not one each.
    for event in TextMergeStream::new(Parser::new_ext(&pre.markdown, markdown_options(false))) {
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
            b.feed(event)?;
        }
    }
    b.finish_html();

    let (bullets, ordered) = numbering_definitions();
    let mut docx = base_docx(&cfg.document, &b.geo)
        .add_abstract_numbering(bullets)
        .add_abstract_numbering(ordered)
        .add_numbering(Numbering::new(BULLET_NUM_ID, BULLET_ABSTRACT_ID));
    for n in b.ordered_numberings.drain(..) {
        docx = docx.add_numbering(n);
    }
    for block in b.blocks.drain(..) {
        docx = match block {
            Block::Para(p) => docx.add_paragraph(*p),
            Block::Table(t) => docx.add_table(*t),
        };
    }

    let mut parts = docx.build();
    if !b.omml.is_empty() {
        debug_assert!(cfg.math.mode == MathMode::NativeOmml);
        let xml = String::from_utf8(std::mem::take(&mut parts.document))
            .map_err(|e| CrateError::DocxGenerationError(format!("document.xml is not UTF-8: {e}")))?;
        parts.document = inject_omml(&xml, &b.nonce, &b.omml)?.into_bytes();
    }

    let in_memory = crate::pack::uncompressed_size(&parts);
    if in_memory > cfg.limits.max_uncompressed_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "the document parts total {in_memory} bytes (Limits::max_uncompressed_bytes {})",
            cfg.limits.max_uncompressed_bytes
        )));
    }
    crate::pack::pack(parts, cfg.compression_level, cfg.limits.max_output_bytes, writer)
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
    fn hostile_dimensions_never_produce_a_picture() {
        for (w, h) in [
            (4_000_000_000, 4_000_000_000),
            (u32::MAX, 1),
            (1, u32::MAX),
            (0, 0),
            (20_000, 20_000),
        ] {
            let r = make_picture(
                &Geometry::default(),
                png_header(w, h),
                1,
                Path::new("hostile.png"),
                None,
            );
            assert!(!matches!(r, Ok(Some(_))), "{w}x{h} must not be embedded");
        }
        // Garbage and unsupported formats are "not an image", not a crash.
        assert!(matches!(
            make_picture(
                &Geometry::default(),
                b"<svg xmlns='x'/>".to_vec(),
                96,
                Path::new("a.svg"),
                None
            ),
            Ok(None)
        ));
        assert!(matches!(
            make_picture(&Geometry::default(), Vec::new(), 96, Path::new("empty"), None),
            Ok(None)
        ));
        assert!(matches!(
            make_picture(
                &Geometry::default(),
                vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0],
                96,
                Path::new("bad.jpg"),
                None
            ),
            Ok(None)
        ));
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
