//! Configuration for the conversion pipeline.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::control::CancelToken;
use crate::diagnostics::LossPolicy;
use crate::extend::{validate_fence_language, validate_tag, FenceRenderer, FenceRenderers, ImageResolver, TagStyle};
use crate::highlight::Highlighting;
use crate::links::{LinkOptions, LinkResolver};
use crate::template::ReferenceDocx;

/// How `$...$` and `$$...$$` formulas end up in the `.docx`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MathMode {
    /// Render each formula to a transparent PNG with an external tool and
    /// embed it as a picture. Works for any LaTeX the tool understands, but
    /// the result is not editable in Word. **Runs a TeX engine on document
    /// content**: trusted input only (see [`Config::trusted`]).
    ImageFallback,
    /// Convert LaTeX to OMML in-process and inject native, editable
    /// `<m:oMath>` / `<m:oMathPara>` elements. No external tool required (default).
    #[default]
    NativeOmml,
}

/// An external program invocation. `args` may contain these placeholders:
///
/// * `{input}`  – path of the file holding the source text
/// * `{output}` – path the tool must write the PNG to
/// * `{dpi}`    – target resolution (from [`MathOptions::dpi`] / [`GraphOptions::dpi`])
/// * `{display}` – `1` for display math, `0` for inline math (math only)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandSpec {
    /// Executable name or path.
    pub program: String,
    /// Arguments; may contain the placeholders above.
    pub args: Vec<String>,
}

impl CommandSpec {
    /// A command with the given program and arguments (placeholders allowed).
    pub fn new<S: Into<String>>(program: impl Into<String>, args: impl IntoIterator<Item = S>) -> Self {
        CommandSpec {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// Tool used to turn LaTeX into a PNG in [`MathMode::ImageFallback`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MathImageEngine {
    /// `latex` followed by `dvipng -T tight -bg Transparent` (TeX Live). The
    /// formula is wrapped in a minimal `standalone`-style document.
    LatexDvipng {
        /// The `latex` executable (e.g. `latex`, or a full path).
        latex: String,
        /// The `dvipng` executable.
        dvipng: String,
    },
    /// Any command that reads LaTeX source from `{input}` and writes a PNG to
    /// `{output}` – e.g. a headless-browser/MathJax node script.
    Command(CommandSpec),
}

impl Default for MathImageEngine {
    fn default() -> Self {
        MathImageEngine::LatexDvipng {
            latex: "latex".into(),
            dvipng: "dvipng".into(),
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
/// How formulas are rendered.
pub struct MathOptions {
    /// Native OMML equations or rendered pictures.
    pub mode: MathMode,
    /// The tool used in [`MathMode::ImageFallback`].
    pub engine: MathImageEngine,
    /// Resolution of rendered formulas; also used to size them in the page.
    pub dpi: u32,
}

impl Default for MathOptions {
    fn default() -> Self {
        MathOptions {
            mode: MathMode::default(),
            engine: MathImageEngine::default(),
            dpi: 300,
        }
    }
}

/// Hard ceilings that keep one document from exhausting memory, disk or time.
/// Exceeding any of them is a [`CrateError::LimitExceeded`](crate::CrateError).
///
/// The struct is `#[non_exhaustive]`, so change a preset's limits by assigning to its fields
/// (a struct literal with `..Limits::default()` does not compile outside this crate):
///
/// ```
/// use md2docx::Config;
///
/// let mut cfg = Config::untrusted();
/// cfg.limits.max_input_bytes = 512 * 1024;
/// cfg.limits.max_elements = 20_000;
/// assert!(cfg.validate().is_ok());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Limits {
    /// Size of the Markdown input.
    pub max_input_bytes: usize,
    /// Number of diagrams plus formulas.
    pub max_rendered_blocks: usize,
    /// Number of diagrams (```` ```mermaid ````/```` ```dolphin ```` blocks).
    pub max_diagrams: usize,
    /// Size of one diagram's source text.
    pub max_graph_source_bytes: usize,
    /// Longest code block that is syntax-highlighted; longer ones are drawn plain.
    /// [`Config::untrusted`] sets 64 KiB.
    pub max_highlight_bytes: usize,
    /// Size of one formula's LaTeX source.
    pub max_formula_bytes: usize,
    /// Size of one source image encoding: a file, bytes from an [`ImageResolver`](crate::ImageResolver)
    /// or asset map, or a renderer's output.
    pub max_image_bytes: u64,
    /// Total bytes of all *distinct* source encodings read (an exact repeat of bytes already
    /// read is not counted again).
    pub max_total_image_bytes: u64,
    /// Number of distinct source encodings. Two encodings of the same pixels count as two; an
    /// exact repeat of the same bytes counts once.
    pub max_distinct_images: usize,
    /// Pixels (width x height) of one image, judged from its header before anything is
    /// allocated.
    pub max_image_pixels: u64,
    /// Longest side of one image, in pixels.
    pub max_image_dimension: u32,
    /// Total pixels decoded across the document: **actual decode work**. Every distinct source
    /// encoding is charged when it is decoded, even if it normalises to pixels already seen;
    /// only an exact repeat of bytes already decoded reuses the earlier result for free.
    pub max_total_decode_pixels: u64,
    /// Total bytes of the normalised PNGs that end up in the document, counting each distinct
    /// picture once.
    pub max_total_normalized_bytes: u64,
    /// Memory a single image decode may allocate.
    pub max_decode_alloc_bytes: u64,
    /// Total size of the scratch directory while external tools run.
    pub max_workspace_bytes: u64,
    /// Size of the finished (compressed) `.docx`, enforced exactly while it is
    /// written; [`Config::untrusted`] sets 50 MiB.
    pub max_output_bytes: u64,
    /// Paragraphs, text runs, table cells and list instances in the document.
    /// Each is built as an object that costs a few KiB until the document is
    /// packaged (1M paragraphs peaked above 7 GiB), so this is the limit that
    /// bounds memory *while building*; [`Config::untrusted`] sets 60 000.
    pub max_elements: usize,
    /// Size of the document's XML parts and pictures before compression, held in
    /// memory while the archive is packaged; checked before packaging.
    /// [`Config::untrusted`] sets 64 MiB. This bounds what readers must parse,
    /// not peak memory during building (see [`Limits::max_elements`]).
    pub max_uncompressed_bytes: u64,
    /// Total wall-clock time one conversion may spend in external tools, on top
    /// of the per-tool [`Config::tool_timeout`].
    pub max_total_tool_time: Duration,
    /// Wall-clock time for the whole conversion, checked at every checkpoint (see
    /// [`CancelToken`](crate::CancelToken)); `None` for no limit. Exceeding it is
    /// [`CrateError::TimedOut`](crate::CrateError). It cannot interrupt a synchronous callback
    /// that does not return; run the conversion in a worker you can terminate for a hard limit.
    /// [`Config::untrusted`] sets 120 s.
    pub max_conversion_time: Option<Duration>,
}

impl Limits {
    /// Ceilings for Markdown you wrote or fully trust: large inputs, many diagrams and
    /// images. Not suitable for untrusted input.
    pub fn trusted() -> Self {
        const MIB: u64 = 1024 * 1024;
        Limits {
            max_input_bytes: 10 * 1024 * 1024,
            max_rendered_blocks: 500,
            max_diagrams: 100,
            max_graph_source_bytes: 64 * 1024,
            max_highlight_bytes: 256 * 1024,
            max_formula_bytes: 16 * 1024,
            max_image_bytes: 25 * MIB,
            max_total_image_bytes: 150 * MIB,
            max_distinct_images: 1_000,
            max_image_pixels: 50_000_000,
            max_image_dimension: 32_768,
            max_total_decode_pixels: 1_000_000_000,
            max_total_normalized_bytes: 256 * MIB,
            max_decode_alloc_bytes: 256 * MIB,
            max_workspace_bytes: 256 * MIB,
            max_output_bytes: 256 * MIB,
            max_elements: 500_000,
            max_uncompressed_bytes: 1024 * MIB,
            max_total_tool_time: Duration::from_secs(300),
            max_conversion_time: None,
        }
    }
}

/// The default limits are the tight ones suited to untrusted Markdown (the same as
/// [`Config::untrusted`] uses); use [`Limits::trusted`] for your own documents.
impl Default for Limits {
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Limits {
            max_input_bytes: 2 * 1024 * 1024,
            max_rendered_blocks: 200,
            max_highlight_bytes: 64 * 1024,
            max_formula_bytes: 4 * 1024,
            max_image_bytes: 5 * MIB,
            max_total_image_bytes: 25 * MIB,
            max_distinct_images: 100,
            max_image_pixels: 16_000_000,
            max_image_dimension: 16_384,
            max_total_decode_pixels: 64_000_000,
            max_total_normalized_bytes: 25 * MIB,
            max_decode_alloc_bytes: 128 * MIB,
            max_output_bytes: 50 * MIB,
            max_elements: 60_000,
            max_uncompressed_bytes: 64 * MIB,
            max_conversion_time: Some(Duration::from_secs(120)),
            ..Limits::trusted()
        }
    }
}

/// Which local files a Markdown image reference may pull into the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ImageAccess {
    /// Never read files; images become `[alt text]`.
    None,
    /// Only files that resolve (after symlinks) inside [`Config::base_dir`]
    /// (default: the current directory). Anything else is an error. This is the default of the
    /// enum and of [`Config::trusted`]; [`Config::default`] and [`Config::untrusted`] use `None`.
    #[default]
    BaseDir,
    /// Any readable file. Only for Markdown you wrote yourself.
    Any,
}

/// How pictures are stored in the `.docx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ImageEncoding {
    /// Decode every picture and write it again as a clean PNG. Appended data, metadata (EXIF,
    /// GPS, ICC profiles, comments) and anything a decoder ignores do not reach the document, and
    /// a camera's EXIF orientation is applied. Photographs get several times larger. The default.
    #[default]
    Png,
    /// Embed the file's own bytes, **1:1**: a PNG, JPEG or GIF is neither re-encoded nor stripped,
    /// so a photograph keeps its size, quality and metadata. The picture is still fully decoded
    /// first, so a truncated or corrupt file, and every image limit, are judged as before; but what
    /// the decoder ignores (appended data, EXIF with a location, comment segments) is carried into
    /// the document, and the EXIF orientation is **not** applied (whether Word honours it is up
    /// to Word). Use it for Markdown and images you trust, or when you strip them elsewhere.
    /// [`Limits::max_total_normalized_bytes`] then budgets the original bytes.
    Original,
}

/// Which program renders ```` ```mermaid ```` blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MermaidEngine {
    /// `merman-cli`: a headless Rust implementation of Mermaid. No browser and
    /// no JavaScript; it enforces its own layout and raster ceilings and does
    /// not read local files or use the network for diagrams. Install with
    /// `cargo install merman-cli`.
    #[default]
    Merman,
    /// `mmdc`, the official Mermaid CLI. It drives a headless Chrome, so it is
    /// the reference renderer but a much larger attack surface.
    Mmdc,
}

impl MermaidEngine {
    /// The command line for this engine: PNG at 3x scale (288 dpi) on a
    /// transparent background.
    pub fn command(self) -> CommandSpec {
        match self {
            MermaidEngine::Merman => CommandSpec::new(
                "merman-cli",
                [
                    "render",
                    "{input}",
                    "-o",
                    "{output}",
                    "-f",
                    "png",
                    "-s",
                    "3",
                    "-b",
                    "transparent",
                    "-q",
                    // Always treat the input as Mermaid source, never as an SVG document.
                    "--input-kind",
                    "mermaid",
                    // The default (deterministic) text metrics are approximate and clip
                    // labels in some boxes; measuring with real fonts fixes that.
                    "--font-measurement",
                    "native",
                ],
            ),
            MermaidEngine::Mmdc => CommandSpec::new(
                "mmdc",
                ["-i", "{input}", "-o", "{output}", "-s", "3", "-b", "transparent"],
            ),
        }
    }
}

/// What happens when a fenced block's renderer fails: the tool is missing, rejects the
/// diagram text, times out, or a registered [`FenceRenderer`](crate::FenceRenderer) errors.
/// Limit errors ([`CrateError::LimitExceeded`](crate::CrateError)) are never softened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum DiagramFailure {
    /// The conversion fails with the renderer's error (default): nothing is hidden.
    #[default]
    Error,
    /// The block stays an ordinary code block and the conversion carries on. Good for
    /// documents from other people, where one bad diagram should not lose the rest.
    KeepAsCode,
}

/// How diagrams are rendered.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GraphOptions {
    /// What to do when a block's renderer fails (default: fail the conversion).
    pub on_failure: DiagramFailure,
    /// Render `mermaid`/`dolphin` fences with external commands. When `false` (the default)
    /// they stay ordinary code blocks and no diagram tool is ever started; a
    /// [`FenceRenderer`] you register for the language still applies.
    pub enabled: bool,
    /// Treat diagram text as hostile. Requires the stock `merman-cli` command
    /// ([`MermaidEngine::Merman`], optionally with a different program path):
    /// no browser is involved, and merman's raster pipeline loads neither local
    /// files nor the network (verified, see the README). In this mode the tool
    /// also runs with a scrubbed environment, and `dolphin` fences (an
    /// arbitrary external command) stay code. Set by
    /// [`Config::untrusted_with_diagrams`].
    pub hardened: bool,
    /// Renderer for ```` ```mermaid ```` blocks. Default:
    /// [`MermaidEngine::Merman`]; use [`GraphOptions::use_engine`] to switch,
    /// or assign any [`CommandSpec`] that writes a PNG.
    pub mermaid: CommandSpec,
    /// Renderer for ```` ```dolphin ```` blocks. There is no de-facto standard
    /// Dolphin CLI, so this default (`dolphin -i {input} -o {output}`) is an
    /// assumption; override it for your tool.
    pub dolphin: CommandSpec,
    /// Effective resolution of the rendered PNGs, used to size them in the
    /// page. A 3x render (`-s 3` for both engines) is 3 x 96 = 288 dpi.
    pub dpi: u32,
}

impl GraphOptions {
    /// True when [`GraphOptions::mermaid`] is the stock merman command line,
    /// possibly with a different program path (e.g. `/opt/bin/merman-cli`).
    pub fn is_stock_merman(&self) -> bool {
        let stock = MermaidEngine::Merman.command();
        let base = self.mermaid.program.rsplit(['/', '\\']).next().unwrap_or("");
        let stem = base.strip_suffix(".exe").unwrap_or(base);
        stem == "merman-cli" && self.mermaid.args == stock.args
    }

    /// Render Mermaid with `engine` (replaces [`GraphOptions::mermaid`]).
    pub fn use_engine(&mut self, engine: MermaidEngine) {
        self.mermaid = engine.command();
    }
}

impl Default for GraphOptions {
    fn default() -> Self {
        GraphOptions {
            on_failure: DiagramFailure::default(),
            enabled: false,
            hardened: false,
            mermaid: MermaidEngine::default().command(),
            dolphin: CommandSpec::new("dolphin", ["-i", "{input}", "-o", "{output}"]),
            dpi: 288,
        }
    }
}

/// Paper size of the generated document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PageSize {
    /// 210 x 297 mm.
    #[default]
    A4,
    /// 8.5 x 11 in.
    Letter,
    /// 8.5 x 14 in.
    Legal,
    /// Explicit size in twentieths of a point (1 inch = 1440). Use a larger
    /// width than height for landscape.
    Custom {
        /// Page width, 2880 (2 in) to 31680 (22 in).
        width_twips: u32,
        /// Page height, 2880 (2 in) to 31680 (22 in).
        height_twips: u32,
    },
}

impl PageSize {
    /// `(width, height)` in twips.
    pub fn twips(self) -> (u32, u32) {
        match self {
            PageSize::A4 => (11906, 16838),
            PageSize::Letter => (12240, 15840),
            PageSize::Legal => (12240, 20160),
            PageSize::Custom {
                width_twips,
                height_twips,
            } => (width_twips, height_twips),
        }
    }
}

/// How a code block (fenced, indented or HTML `<pre>`) is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CodeBlockLayout {
    /// One shaded paragraph per source line. Pasted into a plain-text editor,
    /// each line becomes a line.
    #[default]
    LinePerParagraph,
    /// The whole block is one shaded paragraph, lines separated by line breaks.
    /// Triple-click selects the entire block, which makes it easy to copy; the
    /// text is the raw source with spaces and tabs kept. Word puts a plain
    /// newline on the clipboard for each line break, but some applications
    /// show it as a vertical-tab character: try it with your readers' tools.
    SingleParagraph,
}

/// A table of contents placed at the start of the document.
///
/// It is **static**: one line per heading, each a link to the heading, indented by level, with no
/// page numbers. Word does not have to update fields, ask a question when the file opens, or
/// disagree with another word processor about pagination, and the entries stay correct however
/// the file is re-saved. (A field-based contents with page numbers needs Word to compute it.)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TableOfContents {
    /// Title line above the entries; empty for none (default `Contents`).
    pub title: String,
    /// Deepest heading level listed, 1 to 6 (default 3).
    pub max_level: u8,
}

impl TableOfContents {
    /// A contents titled `Contents`, listing headings down to level 3.
    pub fn new() -> Self {
        TableOfContents {
            title: "Contents".into(),
            max_level: 3,
        }
    }

    /// Set the title line.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the deepest heading level listed.
    pub fn max_level(mut self, level: u8) -> Self {
        self.max_level = level;
        self
    }
}

impl Default for TableOfContents {
    fn default() -> Self {
        Self::new()
    }
}

/// Look of the generated document.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DocumentOptions {
    /// Paper size (default A4).
    pub page: PageSize,
    /// Margin on all four sides, in twips (default 1440 = 1 inch).
    pub margin_twips: u32,
    /// Body font family (default `Calibri`).
    pub body_font: String,
    /// Body size in half-points: 22 = 11 pt (default), range 12..=144.
    pub body_size_half_points: u32,
    /// Font for code (default `Consolas`).
    pub code_font: String,
    /// Heading colour as six hex digits, e.g. `1F3864` (default).
    pub heading_color: String,
    /// Background of code blocks and inline code, six hex digits (default `F2F2F2`).
    pub code_background: String,
    /// Colour of hyperlinks, six hex digits (default `0563C1`).
    pub link_color: String,
    /// Colour of the bar beside block quotes, six hex digits (default `A6A6A6`).
    pub quote_bar_color: String,
    /// Layout of code blocks (default one paragraph per line).
    pub code_blocks: CodeBlockLayout,
    /// Show the page number, centred in the footer (default off).
    pub page_numbers: bool,
    /// Number headings `1`, `1.1`, `1.1.1`, ... with Word's multilevel numbering (default off).
    /// Assumes the headings start at level 1: Word and LibreOffice number a skipped level
    /// differently (`0.1` and `1.1`).
    pub number_headings: bool,
    /// A static, linked table of contents at the start (default none). See [`TableOfContents`].
    pub table_of_contents: Option<TableOfContents>,
    /// Document title for the file's properties. Falls back to `title:` in the
    /// document's front matter. At most 200 printable characters.
    pub title: Option<String>,
    /// Document author for the file's properties. Falls back to `author:` in the front
    /// matter; otherwise "unknown". At most 200 printable characters.
    pub author: Option<String>,
    /// The "created" and "modified" time written to the file's properties. `None` (the default)
    /// uses the time of the conversion, so the same input gives different bytes on each run; set
    /// it (for example from `SOURCE_DATE_EPOCH`) for byte-identical, cacheable output. Must lie
    /// between 1970 and the year 9999.
    pub timestamp: Option<std::time::SystemTime>,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        DocumentOptions {
            page: PageSize::default(),
            margin_twips: 1440,
            body_font: "Calibri".into(),
            body_size_half_points: 22,
            code_font: "Consolas".into(),
            heading_color: "1F3864".into(),
            code_background: "F2F2F2".into(),
            link_color: "0563C1".into(),
            quote_bar_color: "A6A6A6".into(),
            code_blocks: CodeBlockLayout::default(),
            page_numbers: false,
            number_headings: false,
            table_of_contents: None,
            title: None,
            author: None,
            timestamp: None,
        }
    }
}

impl DocumentOptions {
    /// Check the values; the error text names the offending option.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(t) = self.timestamp {
            let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs());
            if !secs.is_ok_and(|s| s < 253_402_300_800) {
                return Err("timestamp must lie between 1970 and the year 9999".into());
            }
        }
        if let Some(toc) = &self.table_of_contents {
            if !(1..=6).contains(&toc.max_level) {
                return Err(format!(
                    "table of contents max_level {} must be between 1 and 6",
                    toc.max_level
                ));
            }
            if toc.title.chars().count() > 200
                || toc
                    .title
                    .chars()
                    .any(|c| c.is_control() || !crate::safety::is_xml_char(c))
            {
                return Err("table of contents title must be at most 200 printable characters".into());
            }
        }
        let (w, h) = self.page.twips();
        if !(2880..=31680).contains(&w) || !(2880..=31680).contains(&h) {
            return Err(format!(
                "page size {w} x {h} twips must be between 2 and 22 inches on each side"
            ));
        }
        if u64::from(self.margin_twips) * 2 + 1440 > u64::from(w.min(h)) {
            return Err(format!(
                "margin of {} twips leaves less than 1 inch of text area on a {w} x {h} page",
                self.margin_twips
            ));
        }
        if !(12..=144).contains(&self.body_size_half_points) {
            return Err(format!(
                "body size {} half-points must be between 12 (6 pt) and 144 (72 pt)",
                self.body_size_half_points
            ));
        }
        for (what, font) in [("body font", &self.body_font), ("code font", &self.code_font)] {
            if font.trim().is_empty()
                || font.chars().count() > 64
                || font.chars().any(|c| c.is_control() || !crate::safety::is_xml_char(c))
            {
                return Err(format!("{what} must be 1 to 64 printable characters"));
            }
        }
        for (what, value) in [("title", &self.title), ("author", &self.author)] {
            if let Some(v) = value {
                if v.chars().count() > 200 || v.chars().any(|c| c.is_control() || !crate::safety::is_xml_char(c)) {
                    return Err(format!("{what} must be at most 200 printable characters"));
                }
            }
        }
        for (what, value) in [
            ("heading colour", &self.heading_color),
            ("code background", &self.code_background),
            ("link colour", &self.link_color),
            ("quote bar colour", &self.quote_bar_color),
        ] {
            if !crate::extend::is_hex_color(value) {
                return Err(format!("{what} `{value}` must be six hex digits like 1F3864"));
            }
        }
        Ok(())
    }
}

/// Top-level configuration for [`crate::markdown_to_docx`].
///
/// The struct is `#[non_exhaustive]`: build one with [`Config::default`]
/// (the untrusted-input policy), [`Config::trusted`] or [`Config::native_math`], then adjust it with the
/// `with_*` methods or by assigning to fields. New options can be added in
/// later versions without breaking your code.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Formula rendering.
    pub math: MathOptions,
    /// Diagram rendering.
    pub graphs: GraphOptions,
    /// Page size, margins and fonts of the generated document.
    pub document: DocumentOptions,
    /// Directory against which relative image paths in the Markdown are
    /// resolved. Defaults to the current directory;
    /// [`crate::markdown_file_to_docx`] sets it to the file's directory.
    pub base_dir: Option<PathBuf>,
    /// Kill an external renderer that runs longer than this.
    pub tool_timeout: Duration,
    /// Hard ceilings on input size, work and output.
    pub limits: Limits,
    /// Which local images may be embedded.
    pub image_access: ImageAccess,
    /// Whether pictures are re-encoded as clean PNGs (the default) or embedded unchanged.
    pub image_encoding: ImageEncoding,
    /// URL schemes a hyperlink may use (lower case, no colon). Links with any
    /// other scheme, or none (relative paths, `#anchors`, UNC paths), are
    /// written as plain text.
    pub allowed_link_schemes: Vec<String>,
    /// Turn raw HTML `<a href>` into hyperlinks (still limited to
    /// [`Config::allowed_link_schemes`]). When `false` the tag is dropped and
    /// only its text stays. `true` in [`Config::trusted`]; `false` in [`Config::default`] and
    /// [`Config::untrusted`].
    pub html_links: bool,
    /// Deflate level of the finished `.docx`: 0 stores it uncompressed, 1 is
    /// fastest, 9 smallest. Default 6. Pictures are always stored (they are
    /// compressed already).
    pub compression_level: u32,
    /// Colour code blocks by language. `None` (the default) draws them plain.
    /// See [`Highlighting`].
    pub highlighting: Option<Highlighting>,
    /// Styles for HTML tags outside the built-in set (or overriding it), by
    /// lower-case tag name. See [`TagStyle`]. Empty by default.
    pub html_tags: BTreeMap<String, TagStyle>,
    /// Renderers for fenced code blocks, by language. See [`FenceRenderer`].
    /// Empty by default.
    pub fence_renderers: FenceRenderers,
    /// Directory under which the conversion creates its private scratch directory (rendered
    /// files, tool logs, tool inputs). `None` uses the system temporary directory. Set it to a
    /// host-owned job directory so every temporary artifact is inside storage the host accounts
    /// for, limits and recovers; no environment variable is touched. The directory must exist; it
    /// is trusted configuration, never document-controlled. See [`crate::sweep_scratch`].
    pub scratch_dir: Option<PathBuf>,
    /// A token the host can use to stop a running conversion; see [`CancelToken`](crate::CancelToken).
    pub cancel: Option<CancelToken>,
    /// Take the document's look (styles and theme) from a `.docx` you supply instead of the
    /// built-in one; see [`ReferenceDocx`]. `DocumentOptions` fonts and colours then only apply to
    /// styles the reference does not define. Trusted configuration.
    pub reference_docx: Option<Arc<ReferenceDocx>>,
    /// Read `^x^` as superscript and `~x~` as subscript (`~~x~~` stays strikethrough). Off by
    /// default: without it a single `~x~` is strikethrough, so turning this on changes what such
    /// existing documents mean, and the markers must stand at word boundaries (`H ~2~ O`). The HTML `<sup>`/`<sub>` tags always work.
    pub superscript_subscript: bool,
    /// Turn bare `http://`, `https://` and `www.` addresses in running text into hyperlinks
    /// (GitHub's "autolink literals"), still limited to [`Config::allowed_link_schemes`]. Off by
    /// default: nothing becomes a link unless the author wrote link syntax.
    pub autolink_bare_urls: bool,
    /// What to do with a document that cannot be converted faithfully; see [`LossPolicy`].
    /// Default [`LossPolicy::Allow`]: degrade as documented and report. A host that must not
    /// deliver a degraded document sets [`LossPolicy::Reject`].
    pub loss_policy: LossPolicy,
    /// Supplies image bytes by key, chosen by the host; never reads a path or the network. See
    /// [`ImageResolver`]. Consulted before [`Config::image_access`], so it works with
    /// [`ImageAccess::None`].
    pub image_resolver: Option<Arc<dyn ImageResolver>>,
    /// Links to files on disk; see [`LinkOptions`]. Off by default, in every preset: a relative
    /// path is shown as text.
    pub links: LinkOptions,
    /// Decides links before the built-in rules; see [`LinkResolver`].
    pub link_resolver: Option<Arc<dyn LinkResolver>>,
}

/// The default is the **untrusted-input policy**: [`Config::untrusted`]. A host that wants
/// anything more (TeX, external diagram renderers, local files) has to say so by starting
/// from [`Config::trusted`] or setting the fields itself.
impl Default for Config {
    fn default() -> Self {
        Config::untrusted()
    }
}

impl Config {
    /// Check every option; called by the conversion functions before any work is
    /// done. The error names the offending option.
    pub fn validate(&self) -> crate::error::Result<()> {
        let bad = |m: String| crate::error::CrateError::InvalidConfig(m);
        self.document.validate().map_err(bad)?;
        if !(1..=2400).contains(&self.math.dpi) || !(1..=2400).contains(&self.graphs.dpi) {
            return Err(bad("dpi values must be between 1 and 2400".into()));
        }
        if self.graphs.enabled && self.graphs.hardened && !self.graphs.is_stock_merman() {
            return Err(bad(
                "hardened diagram mode requires the stock merman-cli command line; \
                 use `use_engine(MermaidEngine::Merman)` (a different program path is fine)"
                    .into(),
            ));
        }
        if self
            .allowed_link_schemes
            .iter()
            .any(|s| s.is_empty() || s.contains(':'))
        {
            return Err(bad(
                "allowed_link_schemes entries must be bare scheme names like `https`".into(),
            ));
        }
        self.links.validate().map_err(bad)?;
        if self.scratch_dir.as_ref().is_some_and(|d| d.as_os_str().is_empty()) {
            return Err(bad("scratch_dir must not be an empty path".into()));
        }
        if self.limits.max_conversion_time.is_some_and(|d| d.is_zero()) {
            return Err(bad("max_conversion_time must be longer than zero".into()));
        }
        if self.compression_level > 9 {
            return Err(bad(format!(
                "compression_level {} must be between 0 (stored) and 9 (smallest)",
                self.compression_level
            )));
        }
        if let Some(h) = &self.highlighting {
            h.theme.validate().map_err(bad)?;
            for lang in h.languages.keys() {
                validate_fence_language(lang).map_err(bad)?;
            }
            if h.languages.len() > 64 {
                return Err(bad("at most 64 per-language highlighters may be registered".into()));
            }
        }
        if self.html_tags.len() > 64 {
            return Err(bad("at most 64 custom html tags may be registered".into()));
        }
        for (name, style) in &self.html_tags {
            validate_tag(name, style).map_err(bad)?;
        }
        if self.fence_renderers.len() > 32 {
            return Err(bad("at most 32 fence renderers may be registered".into()));
        }
        for lang in self.fence_renderers.languages() {
            validate_fence_language(lang).map_err(bad)?;
        }
        Ok(())
    }

    /// Choose native OMML or rendered-picture math.
    pub fn with_math_mode(mut self, mode: MathMode) -> Self {
        self.math.mode = mode;
        self
    }

    /// Set the directory relative image paths resolve against.
    pub fn with_base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// Choose which local images may be embedded.
    pub fn with_image_access(mut self, access: ImageAccess) -> Self {
        self.image_access = access;
        self
    }

    /// Choose whether pictures are re-encoded as PNG or embedded 1:1 (see [`ImageEncoding`]).
    pub fn with_image_encoding(mut self, encoding: ImageEncoding) -> Self {
        self.image_encoding = encoding;
        self
    }

    /// Replace the limits.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Fix the document's created/modified time, making the output reproducible
    /// ([`DocumentOptions::timestamp`]).
    pub fn with_timestamp(mut self, timestamp: std::time::SystemTime) -> Self {
        self.document.timestamp = Some(timestamp);
        self
    }

    /// Replace the page and font options.
    pub fn with_document(mut self, document: DocumentOptions) -> Self {
        self.document = document;
        self
    }

    /// Choose the Mermaid renderer.
    pub fn with_mermaid_engine(mut self, engine: MermaidEngine) -> Self {
        self.graphs.use_engine(engine);
        self
    }

    /// Turn relative links into hyperlinks for local files; see [`LinkOptions`].
    pub fn with_local_links(mut self, local: crate::links::LocalLinks) -> Self {
        self.links.local = local;
        self
    }

    /// Replace the whole link configuration.
    pub fn with_link_options(mut self, links: LinkOptions) -> Self {
        self.links = links;
        self
    }

    /// Decide links yourself, before the built-in rules; see [`LinkResolver`].
    pub fn with_link_resolver(mut self, resolver: impl LinkResolver + 'static) -> Self {
        self.link_resolver = Some(Arc::new(resolver));
        self
    }

    /// Choose what happens to a document that cannot be converted faithfully.
    pub fn with_loss_policy(mut self, policy: LossPolicy) -> Self {
        self.loss_policy = policy;
        self
    }

    /// Style the document from a reference `.docx` (see [`ReferenceDocx`]). The reference is read
    /// and checked now, so a bad one is an error here rather than during a conversion.
    pub fn with_reference_docx(mut self, docx: &[u8]) -> crate::error::Result<Self> {
        self.reference_docx = Some(Arc::new(ReferenceDocx::from_bytes(docx)?));
        Ok(self)
    }

    /// Read `^x^` as superscript and `~x~` as subscript (see [`Config::superscript_subscript`]).
    pub fn with_superscript_subscript(mut self, on: bool) -> Self {
        self.superscript_subscript = on;
        self
    }

    /// Turn bare web addresses into hyperlinks (see [`Config::autolink_bare_urls`]).
    pub fn with_autolink_bare_urls(mut self, on: bool) -> Self {
        self.autolink_bare_urls = on;
        self
    }

    /// Let the host supply image bytes by key (see [`ImageResolver`] and [`crate::AssetMap`]).
    /// Replaces an earlier resolver.
    pub fn with_image_resolver(mut self, resolver: impl ImageResolver + 'static) -> Self {
        self.image_resolver = Some(Arc::new(resolver));
        self
    }

    /// Create the conversion's scratch directory under `dir` instead of the system temporary
    /// directory (see [`Config::scratch_dir`]).
    pub fn with_scratch_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.scratch_dir = Some(dir.into());
        self
    }

    /// Let the host stop a running conversion (see [`CancelToken`](crate::CancelToken)).
    pub fn with_cancel_token(mut self, token: CancelToken) -> Self {
        self.cancel = Some(token);
        self
    }

    /// Set the wall-clock limit for the whole conversion ([`Limits::max_conversion_time`]).
    pub fn with_deadline(mut self, limit: Duration) -> Self {
        self.limits.max_conversion_time = Some(limit);
        self
    }

    /// Set the per-tool timeout.
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// Colour code blocks with `highlighting` (see [`Highlighting`] and [`crate::Theme`]).
    pub fn with_highlighting(mut self, highlighting: Highlighting) -> Self {
        self.highlighting = Some(highlighting);
        self
    }

    /// Set the deflate level of the finished `.docx` (0 to 9, default 6).
    pub fn with_compression_level(mut self, level: u32) -> Self {
        self.compression_level = level;
        self
    }

    /// Turn raw HTML `<a href>` links on or off.
    pub fn with_html_links(mut self, on: bool) -> Self {
        self.html_links = on;
        self
    }

    /// Style the HTML tag `name` (lower case). Replaces an earlier style for
    /// the same tag; an invalid name or colour is reported by
    /// [`Config::validate`] (so by the conversion functions), not here.
    pub fn with_html_tag(mut self, name: impl Into<String>, style: TagStyle) -> Self {
        self.html_tags.insert(name.into(), style);
        self
    }

    /// Render fenced code blocks tagged `lang` with `renderer` instead of
    /// leaving them as code. Replaces an earlier renderer for the same
    /// language. Registering `mermaid` or `dolphin` replaces the external
    /// command for it. A closure `|source: &str| -> Result<Vec<u8>, FenceError>`
    /// works as a renderer.
    pub fn with_fence_renderer(mut self, lang: impl AsRef<str>, renderer: impl FenceRenderer + 'static) -> Self {
        let renderer: Arc<dyn FenceRenderer> = Arc::new(renderer);
        self.fence_renderers
            .0
            .insert(lang.as_ref().to_ascii_lowercase(), renderer);
        self
    }

    /// Strictest preset for Markdown from people you do not trust (uploads,
    /// web forms, multi-tenant services), and what [`Config::default`] returns: native
    /// math only, so no TeX process; diagram fences are left as code, so no external
    /// renderer; no local files are read; raw HTML `<a>` links are dropped; tight
    /// limits. No subprocess is ever spawned and nothing outside the process is read.
    ///
    /// This is a **library-level policy**: it bounds what the converter will do with
    /// the document. It does not isolate the process. A host that must survive hostile
    /// input still runs the conversion in a resource-limited worker (memory, CPU, wall
    /// clock, file descriptors, no network) and terminates it from outside; see
    /// `SECURITY.md`.
    pub fn untrusted() -> Self {
        let mut c = Config::trusted();
        c.math.mode = MathMode::NativeOmml;
        c.graphs.enabled = false;
        c.image_access = ImageAccess::None;
        c.html_links = false;
        c.limits = Limits::default();
        c
    }

    /// [`Config::untrusted`] plus Mermaid diagrams rendered by `merman-cli`.
    ///
    /// Still no browser, no local files, no network. This does start one
    /// external process per diagram (`merman-cli`, which must be installed),
    /// so it is bounded tighter than the default: at most 20 diagrams of 16 KiB
    /// each, 20 s per diagram and 60 s in total, run with a scrubbed
    /// environment. `dolphin` fences stay code. Prefer plain
    /// [`Config::untrusted`] if you do not need diagrams, or an in-process
    /// [`FenceRenderer`] if your worker may not spawn processes.
    pub fn untrusted_with_diagrams() -> Self {
        let mut c = Config::untrusted();
        c.graphs.enabled = true;
        c.graphs.hardened = true;
        c.graphs.use_engine(MermaidEngine::Merman);
        c.tool_timeout = Duration::from_secs(20);
        c.limits.max_diagrams = 20;
        c.limits.max_graph_source_bytes = 16 * 1024;
        c.limits.max_total_tool_time = Duration::from_secs(60);
        c
    }

    /// Configuration for documents you wrote or fully trust: math as pictures through a local
    /// TeX install (the original specification's default), diagrams through `merman-cli`,
    /// local images from [`Config::base_dir`], raw HTML links on, generous limits.
    ///
    /// **Do not use this for input from other people.** It runs external programs on document
    /// content and reads local files.
    pub fn trusted() -> Self {
        Config {
            math: MathOptions {
                mode: MathMode::ImageFallback,
                ..MathOptions::default()
            },
            graphs: GraphOptions {
                enabled: true,
                ..GraphOptions::default()
            },
            document: DocumentOptions::default(),
            base_dir: None,
            tool_timeout: Duration::from_secs(60),
            limits: Limits::trusted(),
            image_access: ImageAccess::BaseDir,
            image_encoding: ImageEncoding::Png,
            allowed_link_schemes: ["http", "https", "mailto"].map(String::from).to_vec(),
            html_links: true,
            compression_level: 6,
            highlighting: None,
            html_tags: BTreeMap::new(),
            fence_renderers: FenceRenderers::default(),
            scratch_dir: None,
            cancel: None,
            reference_docx: None,
            superscript_subscript: false,
            autolink_bare_urls: false,
            loss_policy: LossPolicy::default(),
            image_resolver: None,
            links: LinkOptions::default(),
            link_resolver: None,
        }
    }

    /// [`Config::trusted`] with native, editable OMML equations, so no TeX is needed.
    ///
    /// **This is not an untrusted-input preset.** Choosing native math removes the TeX
    /// process only; this configuration still reads images from `base_dir`, renders diagrams
    /// with external programs and accepts raw HTML links. Use [`Config::untrusted`] (the
    /// default) for input you do not control.
    pub fn native_math() -> Self {
        let mut c = Config::trusted();
        c.math.mode = MathMode::NativeOmml;
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardened_diagram_preset_is_tight_and_validated() {
        let c = Config::untrusted_with_diagrams();
        assert!(c.graphs.enabled && c.graphs.hardened && c.graphs.is_stock_merman());
        assert!(c.validate().is_ok());
        assert_eq!(c.math.mode, MathMode::NativeOmml);
        assert_eq!(c.image_access, ImageAccess::None);
        assert!(!c.html_links);
        assert_eq!(
            (c.limits.max_diagrams, c.limits.max_graph_source_bytes),
            (20, 16 * 1024)
        );
        assert!(c.tool_timeout <= Duration::from_secs(20));
        // A different path to the same tool is fine; a different command is not.
        let mut ok = c.clone();
        ok.graphs.mermaid.program = "/opt/tools/merman-cli".into();
        assert!(ok.validate().is_ok());
        let mut win = c.clone();
        win.graphs.mermaid.program = "C:\\tools\\merman-cli.exe".into();
        assert!(win.validate().is_ok());
        for evil in ["mmdc", "sh", "merman-cli-fork"] {
            let mut bad = c.clone();
            bad.graphs.mermaid.program = evil.into();
            assert!(
                matches!(bad.validate(), Err(crate::CrateError::InvalidConfig(_))),
                "{evil}"
            );
        }
        let mut extra = c.clone();
        extra.graphs.mermaid.args.push("--allow-network".into());
        assert!(extra.validate().is_err(), "extra flags change what the tool may do");
        // Plain untrusted has diagrams off, so nothing to check.
        assert!(!Config::untrusted().graphs.enabled);
    }

    #[test]
    fn mermaid_defaults_to_merman_with_native_fonts_and_can_switch() {
        let mut g = GraphOptions::default();
        assert_eq!(g.mermaid.program, "merman-cli");
        for needed in ["render", "{input}", "{output}", "png", "transparent", "native"] {
            assert!(
                g.mermaid.args.iter().any(|a| a == needed),
                "missing `{needed}`: {:?}",
                g.mermaid.args
            );
        }
        g.use_engine(MermaidEngine::Mmdc);
        assert_eq!(g.mermaid.program, "mmdc");
        assert!(g.mermaid.args.windows(2).any(|w| w == ["-s", "3"]));
        g.use_engine(MermaidEngine::Merman);
        assert_eq!(g.mermaid, MermaidEngine::Merman.command());
        // Both engines render at 3x, which is what `GraphOptions::dpi` assumes (3 x 96).
        for e in [MermaidEngine::Merman, MermaidEngine::Mmdc] {
            assert!(e.command().args.windows(2).any(|w| w == ["-s", "3"]), "{e:?}");
        }
        assert_eq!(GraphOptions::default().dpi, 288);
    }
}
