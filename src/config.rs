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

#[derive(Debug, Clone)]
pub struct GraphOptions {
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            math: MathOptions::default(),
            graphs: GraphOptions::default(),
            base_dir: None,
            tool_timeout: Duration::from_secs(60),
        }
    }
}

impl Config {
    /// Default configuration with native, editable OMML equations.
    pub fn native_math() -> Self {
        let mut c = Config::default();
        c.math.mode = MathMode::NativeOmml;
        c
    }
}
