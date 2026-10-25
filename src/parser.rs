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
use pulldown_cmark::{Alignment, BlockQuoteKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::config::{Config, ImageAccess, MathMode};
use crate::error::{CrateError, Result};
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

const PAGE_WIDTH_TWIPS: u32 = 11906;
const MARGIN_TWIPS: i32 = 1440;
/// Usable text width on A4 with 1" side margins.
const CONTENT_WIDTH_TWIPS: usize = PAGE_WIDTH_TWIPS as usize - 2 * MARGIN_TWIPS as usize;
/// The same width in EMU (1 twip = 635 EMU).
const MAX_IMAGE_WIDTH_EMU: u64 = CONTENT_WIDTH_TWIPS as u64 * 635;
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

fn base_docx() -> Docx {
    let mut docx = Docx::new()
        .default_fonts(fonts("Calibri"))
        .default_size(22)
        .default_line_spacing(LineSpacing::new().after(120))
        .page_size(PAGE_WIDTH_TWIPS, 16838) // A4
        .page_margin(
            PageMargin::new()
                .top(1440)
                .bottom(1440)
                .left(MARGIN_TWIPS)
                .right(MARGIN_TWIPS),
        );
    let heading_sizes = [40usize, 32, 28, 26, 24, 22];
    for (i, size) in heading_sizes.iter().enumerate() {
        docx = docx.add_style(
            Style::new(format!("Heading{}", i + 1), StyleType::Paragraph)
                .name(format!("heading {}", i + 1))
                .size(*size)
                .bold()
                .color("1F3864")
                .outline_lvl(i),
        );
    }
    docx.add_style(
        Style::new("Hyperlink", StyleType::Character)
            .name("Hyperlink")
            .color("0563C1")
            .underline("single"),
    )
    .add_style(
        Style::new("InlineCode", StyleType::Character)
            .name("Inline Code")
            .fonts(fonts("Consolas"))
            .size(20),
    )
    .add_style(
        Style::new("CodeBlock", StyleType::Paragraph)
            .name("Code Block")
            .fonts(fonts("Consolas"))
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

struct ImageCtx {
    dest: String,
    alt: String,
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
    /// Hidden element (`script`...) opened by inline HTML and not yet closed:
    /// text events are skipped until it closes or the paragraph ends.
    hidden: Option<&'static str>,
    underline: usize,
    sup: usize,
    sub: usize,
    mono: usize,
    def_depth: usize,

    blocks: Vec<Block>,
    cur: Option<Vec<Child>>,
    kind: ParaKind,
    bold: usize,
    italic: usize,
    strike: usize,
    link: Option<LinkCtx>,
    image: Option<ImageCtx>,
    code: Option<String>,
    quote_depth: usize,
    lists: Vec<ListCtx>,
    ordered_numberings: Vec<Numbering>,
    next_num_id: usize,
    table: Option<TableState>,
    omml: Vec<OmmlSlot>,
}

fn marker(nonce: &str, slot: usize) -> String {
    format!("SILVERCITYOMML{nonce}X{slot}X")
}

/// Refuse images whose header claims more pixels than this (50 MP; a decoded
/// RGBA frame of that size is 200 MB).
const MAX_IMAGE_PIXELS: u64 = 50_000_000;
/// Allocation ceiling handed to the image decoders.
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
/// Tallest picture we place: the A4 text height (1" margins) in EMU.
const MAX_IMAGE_HEIGHT_EMU: u128 = (16838 - 2 * MARGIN_TWIPS as u128) * 635;

/// Display size in EMU for a `w` x `h` pixel image at `dpi`, shrunk
/// (aspect ratio preserved) to fit the text area. Every step is checked;
/// `None` means the numbers do not fit, never a wrapped or panicking result.
fn display_size_emu(w: u32, h: u32, dpi: u32) -> Option<(u32, u32)> {
    let dpi = u128::from(dpi.max(1));
    let emu = u128::from(EMU_PER_INCH);
    let mut we = u128::from(w).checked_mul(emu)?.checked_div(dpi)?;
    let mut he = u128::from(h).checked_mul(emu)?.checked_div(dpi)?;
    let max_w = u128::from(MAX_IMAGE_WIDTH_EMU);
    if we > max_w {
        he = he.checked_mul(max_w)?.checked_div(we)?;
        we = max_w;
    }
    if he > MAX_IMAGE_HEIGHT_EMU {
        we = we.checked_mul(MAX_IMAGE_HEIGHT_EMU)?.checked_div(he)?;
        he = MAX_IMAGE_HEIGHT_EMU;
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
fn make_picture(bytes: Vec<u8>, dpi: u32, origin: &Path) -> Result<Option<Pic>> {
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
    let (we, he) = display_size_emu(w, h, dpi)
        .ok_or_else(|| CrateError::LimitExceeded(format!("image {} ({w}x{h} px) is out of range", origin.display())))?;
    Ok(Some(Pic::new_with_dimensions(png, w, h).size(we, he)))
}

impl<'a> Builder<'a> {
    fn new(pre: &'a Preprocessed, cfg: &'a Config, nonce: String) -> Self {
        Builder {
            pre,
            cfg,
            base_dir: cfg.base_dir.clone().unwrap_or_else(|| PathBuf::from(".")),
            nonce,
            footnotes: HashMap::new(),
            in_footnote: false,
            footnote_bytes: 0,
            image_bytes: 0,
            html: None,
            hidden: None,
            underline: 0,
            sup: 0,
            sub: 0,
            mono: 0,
            def_depth: 0,
            blocks: Vec::new(),
            cur: None,
            kind: ParaKind::Normal,
            bold: 0,
            italic: 0,
            strike: 0,
            link: None,
            image: None,
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
        run
    }

    fn push(&mut self, child: Child) {
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
        if let Some(t) = self.table.as_mut() {
            if let Some(cell) = t.cell.take() {
                t.cell = Some(cell.add_paragraph(p));
                t.cell_paras += 1;
            }
        } else {
            self.blocks.push(Block::Para(Box::new(p)));
        }
    }

    /// Resolve the children of a finished paragraph into a docx paragraph.
    fn flush(&mut self) {
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
        }
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

    fn attach(&mut self, p: Paragraph, c: Child) -> Paragraph {
        match c {
            Child::Run(r) | Child::Picture { run: r, .. } => p.add_run(r),
            Child::Link(l) => p.add_hyperlink(l),
            Child::Omml { slot, .. } => p.add_run(Run::new().add_text(marker(&self.nonce, slot))),
        }
    }

    /// Indent for block quotes (with a grey bar) and definition-list bodies.
    fn indented(&self, p: Paragraph) -> Paragraph {
        let left = 360 * self.quote_depth + 720 * self.def_depth;
        let mut p = p.indent(Some(i32::try_from(left).unwrap_or(i32::MAX)), None, None, None);
        if self.quote_depth > 0 {
            p.property = p.property.set_borders(
                ParagraphBorders::with_empty().set(
                    ParagraphBorder::new(ParagraphBorderPosition::Left)
                        .val(BorderType::Single)
                        .size(18)
                        .space(8)
                        .color("A6A6A6"),
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
            (Some(ctx), _) if ctx.item_open => Some(720 * (ctx.level as i32 + 1)),
            (_, q) if q > 0 => Some(360 * q as i32),
            _ => None,
        };
        for line in body.split('\n') {
            let mut p = Paragraph::new().style("CodeBlock");
            p.property = p.property.shading(Shading::new().fill("F2F2F2"));
            if let Some(l) = left {
                p = p.indent(Some(l), None, None, None);
            }
            p = p.add_run(Run::new().add_text(sanitize_xml_chars(line.trim_end_matches('\r'))));
            self.emit(p);
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
        self.spend_image_bytes(bytes.len() as u64)?;
        match make_picture(bytes, 96, &path)? {
            Some(pic) => self.push(Child::Picture {
                run: Run::new().add_image(pic),
                kind: AssetKind::Graph,
            }),
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
                self.spend_image_bytes(bytes.len() as u64)?;
                let dpi = match asset.kind {
                    AssetKind::Graph => self.cfg.graphs.dpi,
                    // Formulas are typeset at 12pt; body text is 11pt.
                    _ => self.cfg.math.dpi.saturating_mul(12) / 11,
                };
                match make_picture(bytes, dpi, path)? {
                    Some(pic) => self.push(Child::Picture {
                        run: Run::new().add_image(pic),
                        kind: asset.kind,
                    }),
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
                let start = usize::try_from(start).unwrap_or(1);
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
            match piece {
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
        match name {
            "br" if !closing => self.push(Child::Run(Run::new().add_break(BreakType::TextWrapping))),
            "b" | "strong" => adjust(&mut self.bold, closing),
            "i" | "em" | "cite" | "dfn" | "var" => adjust(&mut self.italic, closing),
            "u" | "ins" => adjust(&mut self.underline, closing),
            "s" | "del" | "strike" => adjust(&mut self.strike, closing),
            "sup" => adjust(&mut self.sup, closing),
            "sub" => adjust(&mut self.sub, closing),
            "code" | "kbd" | "samp" | "tt" => adjust(&mut self.mono, closing),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                if !closing {
                    self.start_para(ParaKind::Heading(name.as_bytes()[1] - b'0'));
                }
            }
            "li" => {
                self.flush();
                if !closing {
                    self.text("\u{2022} ");
                }
            }
            "hr" => {
                self.flush();
                self.rule();
            }
            "td" | "th" if closing => self.text("  "),
            "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "aside" | "main" | "ul" | "ol"
            | "dl" | "dt" | "dd" | "table" | "tr" | "blockquote" | "pre" | "figure" | "figcaption" | "details"
            | "summary" | "center" | "address" => self.flush(),
            "img" if !closing => {
                if let Some(src) = html::attr(attrs, "src").filter(|s| !s.trim().is_empty()) {
                    let alt = html::attr(attrs, "alt").unwrap_or_default();
                    self.image(ImageCtx { dest: src, alt })?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ---- footnotes

    fn footnote_reference(&mut self, label: &str) -> Result<()> {
        let events = match self.footnotes.get(label) {
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
        for ev in events {
            sub.feed(ev)?;
        }
        sub.flush();
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
        let image_ended = matches!(ev, Event::End(TagEnd::Image));
        self.event(ev)?;
        if image_ended {
            if let Some(ctx) = self.image.take() {
                self.image(ctx)?;
            }
        }
        Ok(())
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
                            .shading(Shading::new().fill("F2F2F2"))
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
            Tag::Image { dest_url, .. } => {
                self.image = Some(ImageCtx {
                    dest: dest_url.to_string(),
                    alt: String::new(),
                });
            }
            Tag::Table(aligns) => {
                self.flush();
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
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    let cols = t.aligns.len().max(1);
                    let table = Table::new(t.rows)
                        .set_grid(vec![CONTENT_WIDTH_TWIPS / cols; cols])
                        .width(CONTENT_WIDTH_TWIPS, WidthType::Dxa);
                    self.blocks.push(Block::Table(Box::new(table)));
                }
            }
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
    for ev in Parser::new_ext(markdown, markdown_options(false)) {
        match ev {
            Event::Start(Tag::FootnoteDefinition(label)) => open = Some((label.to_string(), Vec::new())),
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

pub fn build_docx(pre: &Preprocessed, cfg: &Config) -> Result<Vec<u8>> {
    let nonce = crate::renderer::asset_stem("", &[pre.markdown.as_bytes()]).replace('_', "");
    let mut b = Builder::new(pre, cfg, nonce);
    b.footnotes = collect_footnotes(&pre.markdown);

    let mut in_definition = false;
    for event in Parser::new_ext(&pre.markdown, markdown_options(false)) {
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
    b.flush();

    let (bullets, ordered) = numbering_definitions();
    let mut docx = base_docx()
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

    let mut out = Cursor::new(Vec::new());
    parts
        .pack(&mut out)
        .map_err(|e| CrateError::DocxGenerationError(e.to_string()))?;
    Ok(out.into_inner())
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
            let r = make_picture(png_header(w, h), 1, Path::new("hostile.png"));
            assert!(!matches!(r, Ok(Some(_))), "{w}x{h} must not be embedded");
        }
        // Garbage and unsupported formats are "not an image", not a crash.
        assert!(matches!(
            make_picture(b"<svg xmlns='x'/>".to_vec(), 96, Path::new("a.svg")),
            Ok(None)
        ));
        assert!(matches!(make_picture(Vec::new(), 96, Path::new("empty")), Ok(None)));
        assert!(matches!(
            make_picture(vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0], 96, Path::new("bad.jpg")),
            Ok(None)
        ));
    }

    #[test]
    fn display_size_is_bounded_and_keeps_aspect_ratio() {
        // 6in x 3in at 288 dpi, well inside the page: exact.
        assert_eq!(display_size_emu(1728, 864, 288), Some((5_486_400, 2_743_200)));
        // Wider than the page: clamped to the text width, ratio kept.
        let (w, h) = display_size_emu(10_000, 5_000, 96).unwrap();
        assert_eq!(u64::from(w), MAX_IMAGE_WIDTH_EMU);
        assert!((i64::from(w) - 2 * i64::from(h)).abs() <= 1);
        // Very tall and narrow: clamped to the text height.
        let (w, h) = display_size_emu(100, 50_000, 96).unwrap();
        assert_eq!(u128::from(h), MAX_IMAGE_HEIGHT_EMU);
        assert!(w >= 1);
        // Extremes never overflow, wrap, or divide by zero.
        assert!(display_size_emu(u32::MAX, u32::MAX, 0).is_some());
        assert!(display_size_emu(1, 1, u32::MAX).is_some());
        assert_eq!(display_size_emu(0, 0, 96), Some((1, 1)));
    }
}
