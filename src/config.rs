//! Configuration for the conversion pipeline.

use std::path::PathBuf;
use std::time::Duration;

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
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
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
    LatexDvipng { latex: String, dvipng: String },
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
pub struct MathOptions {
    pub mode: MathMode,
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
pub struct Limits {
    /// Size of the Markdown input.
    pub max_input_bytes: usize,
    /// Number of diagrams plus formulas.
    pub max_rendered_blocks: usize,
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
}

impl Default for Limits {
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Limits {
            max_input_bytes: 10 * 1024 * 1024,
            max_rendered_blocks: 500,
            max_graph_source_bytes: 64 * 1024,
            max_formula_bytes: 16 * 1024,
            max_image_bytes: 25 * MIB,
            max_total_image_bytes: 150 * MIB,
            max_workspace_bytes: 256 * MIB,
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

#[derive(Debug, Clone)]
pub struct GraphOptions {
    /// Render `mermaid`/`dolphin` fences. When `false` they stay ordinary code
    /// blocks and no diagram tool is ever started.
    pub enabled: bool,
    /// Renderer for ```` ```mermaid ```` blocks. Default: `mmdc` at 3x scale
    /// with a transparent background.
    pub mermaid: CommandSpec,
    /// Renderer for ```` ```dolphin ```` blocks. There is no de-facto standard
    /// Dolphin CLI, so this default (`dolphin -i {input} -o {output}`) is an
    /// assumption; override it for your tool.
    pub dolphin: CommandSpec,
    /// Effective resolution of the rendered PNGs, used to size them in the
    /// page. `mmdc -s 3` produces 3 x 96 = 288 dpi.
    pub dpi: u32,
}

impl Default for GraphOptions {
    fn default() -> Self {
        GraphOptions {
            enabled: true,
            mermaid: CommandSpec::new(
                "mmdc",
                ["-i", "{input}", "-o", "{output}", "-s", "3", "-b", "transparent"],
            ),
            dolphin: CommandSpec::new("dolphin", ["-i", "{input}", "-o", "{output}"]),
            dpi: 288,
        }
    }
}

/// Top-level configuration for [`crate::markdown_to_docx`].
#[derive(Debug, Clone)]
pub struct Config {
    pub math: MathOptions,
    pub graphs: GraphOptions,
    /// Directory against which relative image paths in the Markdown are
    /// resolved. Defaults to the current directory;
    /// [`crate::markdown_file_to_docx`] sets it to the file's directory.
    pub base_dir: Option<PathBuf>,
    /// Kill an external renderer that runs longer than this.
    pub tool_timeout: Duration,
    pub limits: Limits,
    /// Which local images may be embedded.
    pub image_access: ImageAccess,
    /// URL schemes a hyperlink may use (lower case, no colon). Links with any
    /// other scheme, or none (relative paths, `#anchors`, UNC paths), are
    /// written as plain text.
    pub allowed_link_schemes: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            math: MathOptions::default(),
            graphs: GraphOptions::default(),
            base_dir: None,
            tool_timeout: Duration::from_secs(60),
            limits: Limits::default(),
            image_access: ImageAccess::default(),
            allowed_link_schemes: ["http", "https", "mailto"].map(String::from).to_vec(),
        }
    }
}

impl Config {
    /// Strictest preset for Markdown from people you do not trust (uploads,
    /// web forms, multi-tenant services): native math only, so no TeX process;
    /// diagram fences are left as code, so no browser process; no local files
    /// are read; tighter size limits. No subprocess is ever spawned.
    pub fn untrusted() -> Self {
        let mut c = Config::native_math();
        c.graphs.enabled = false;
        c.image_access = ImageAccess::None;
        c.limits = Limits {
            max_input_bytes: 2 * 1024 * 1024,
            max_rendered_blocks: 200,
            max_formula_bytes: 4 * 1024,
            max_image_bytes: 5 * 1024 * 1024,
            max_total_image_bytes: 25 * 1024 * 1024,
            ..Limits::default()
        };
        c
    }

    /// Default configuration with native, editable OMML equations.
    pub fn native_math() -> Self {
        let mut c = Config::default();
        c.math.mode = MathMode::NativeOmml;
        c
    }
}
