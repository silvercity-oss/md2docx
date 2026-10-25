//! Configuration for the conversion pipeline.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::extend::{validate_fence_language, validate_tag, FenceRenderer, FenceRenderers, TagStyle};

/// How `$...$` and `$$...$$` formulas end up in the `.docx`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MathMode {
    /// Render each formula to a transparent PNG with an external tool and
    /// embed it as a picture. Works for any LaTeX the tool understands, but
    /// the result is not editable in Word.
    #[default]
    ImageFallback,
    /// Convert LaTeX to OMML in-process and inject native, editable
    /// `<m:oMath>` / `<m:oMathPara>` elements. No external tool required.
    NativeOmml,
}

/// An external program invocation. `args` may contain these placeholders:
///
/// * `{input}`  – path of the file holding the source text
/// * `{output}` – path the tool must write the PNG to
/// * `{dpi}`    – target resolution (from [`MathOptions::dpi`] / [`GraphOptions::dpi`])
/// * `{display}` – `1` for display math, `0` for inline math (math only)
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Size of one formula's LaTeX source.
    pub max_formula_bytes: usize,
    /// Size of one embedded image (a file, or a renderer's output).
    pub max_image_bytes: u64,
    /// Total bytes of all embedded images.
    pub max_total_image_bytes: u64,
    /// Total size of the scratch directory while external tools run.
    pub max_workspace_bytes: u64,
    /// Size of the finished (compressed) `.docx`, enforced exactly while it is
    /// written; [`Config::untrusted`] sets 50 MiB.
    pub max_output_bytes: u64,
    /// Size of the document's XML parts and pictures before compression. They
    /// are held in memory (twice, briefly, while the archive is rewritten), so
    /// this bounds peak memory; [`Config::untrusted`] sets 256 MiB. Text
    /// expands about 40x into XML, so a 2 MiB input stays far below that.
    pub max_uncompressed_bytes: u64,
    /// Total wall-clock time one conversion may spend in external tools, on top
    /// of the per-tool [`Config::tool_timeout`].
    pub max_total_tool_time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Limits {
            max_input_bytes: 10 * 1024 * 1024,
            max_rendered_blocks: 500,
            max_diagrams: 100,
            max_graph_source_bytes: 64 * 1024,
            max_formula_bytes: 16 * 1024,
            max_image_bytes: 25 * MIB,
            max_total_image_bytes: 150 * MIB,
            max_workspace_bytes: 256 * MIB,
            max_output_bytes: 256 * MIB,
            max_uncompressed_bytes: 1024 * MIB,
            max_total_tool_time: Duration::from_secs(300),
        }
    }
}

/// Which local files a Markdown image reference may pull into the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageAccess {
    /// Never read files; images become `[alt text]`.
    None,
    /// Only files that resolve (after symlinks) inside [`Config::base_dir`]
    /// (default: the current directory). Anything else is an error.
    #[default]
    BaseDir,
    /// Any readable file. Only for Markdown you wrote yourself.
    Any,
}

/// Which program renders ```` ```mermaid ```` blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// How diagrams are rendered.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GraphOptions {
    /// Render `mermaid`/`dolphin` fences. When `false` they stay ordinary code
    /// blocks and no diagram tool is ever started.
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
            enabled: true,
            hardened: false,
            mermaid: MermaidEngine::default().command(),
            dolphin: CommandSpec::new("dolphin", ["-i", "{input}", "-o", "{output}"]),
            dpi: 288,
        }
    }
}

/// Paper size of the generated document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
        }
    }
}

impl DocumentOptions {
    /// Check the values; the error text names the offending option.
    pub fn validate(&self) -> std::result::Result<(), String> {
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
            if font.trim().is_empty() || font.chars().count() > 64 || font.chars().any(char::is_control) {
                return Err(format!("{what} must be 1 to 64 printable characters"));
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
/// The struct is `#[non_exhaustive]`: build one with [`Config::default`],
/// [`Config::native_math`] or [`Config::untrusted`], then adjust it with the
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
    /// URL schemes a hyperlink may use (lower case, no colon). Links with any
    /// other scheme, or none (relative paths, `#anchors`, UNC paths), are
    /// written as plain text.
    pub allowed_link_schemes: Vec<String>,
    /// Turn raw HTML `<a href>` into hyperlinks (still limited to
    /// [`Config::allowed_link_schemes`]). When `false` the tag is dropped and
    /// only its text stays. Default `true`; [`Config::untrusted`] sets `false`.
    pub html_links: bool,
    /// Deflate level of the finished `.docx`: 0 stores it uncompressed, 1 is
    /// fastest, 9 smallest. Default 6. Pictures are always stored (they are
    /// compressed already).
    pub compression_level: u32,
    /// Styles for HTML tags outside the built-in set (or overriding it), by
    /// lower-case tag name. See [`TagStyle`]. Empty by default.
    pub html_tags: BTreeMap<String, TagStyle>,
    /// Renderers for fenced code blocks, by language. See [`FenceRenderer`].
    /// Empty by default.
    pub fence_renderers: FenceRenderers,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            math: MathOptions::default(),
            graphs: GraphOptions::default(),
            document: DocumentOptions::default(),
            base_dir: None,
            tool_timeout: Duration::from_secs(60),
            limits: Limits::default(),
            image_access: ImageAccess::default(),
            allowed_link_schemes: ["http", "https", "mailto"].map(String::from).to_vec(),
            html_links: true,
            compression_level: 6,
            html_tags: BTreeMap::new(),
            fence_renderers: FenceRenderers::default(),
        }
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
        if self.compression_level > 9 {
            return Err(bad(format!(
                "compression_level {} must be between 0 (stored) and 9 (smallest)",
                self.compression_level
            )));
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

    /// Replace the limits.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
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

    /// Set the per-tool timeout.
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
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
    /// web forms, multi-tenant services): native math only, so no TeX process;
    /// diagram fences are left as code, so no browser process; no local files
    /// are read; tighter size limits. No subprocess is ever spawned.
    pub fn untrusted() -> Self {
        let mut c = Config::native_math();
        c.graphs.enabled = false;
        c.image_access = ImageAccess::None;
        c.html_links = false;
        c.limits = Limits {
            max_input_bytes: 2 * 1024 * 1024,
            max_rendered_blocks: 200,
            max_formula_bytes: 4 * 1024,
            max_image_bytes: 5 * 1024 * 1024,
            max_total_image_bytes: 25 * 1024 * 1024,
            max_output_bytes: 50 * 1024 * 1024,
            max_uncompressed_bytes: 256 * 1024 * 1024,
            ..Limits::default()
        };
        c
    }

    /// [`Config::untrusted`] plus Mermaid diagrams rendered by `merman-cli`.
    ///
    /// Still no browser, no local files, no network. This does start one
    /// external process per diagram (`merman-cli`, which must be installed),
    /// so it is bounded tighter than the default: at most 20 diagrams of 16 KiB
    /// each, 20 s per diagram and 60 s in total, run with a scrubbed
    /// environment. `dolphin` fences stay code. Prefer plain
    /// [`Config::untrusted`] if you do not need diagrams.
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

    /// Default configuration with native, editable OMML equations.
    pub fn native_math() -> Self {
        let mut c = Config::default();
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
