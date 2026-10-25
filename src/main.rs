//! `md2docx`: command-line front end for the Markdown -> .docx converter.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser, ValueEnum};
use silvercity_oss::{
    convert_file, CodeBlockLayout, CommandSpec, Config, DiagramFailure, Highlighting, ImageAccess as LibImageAccess,
    LossPolicy, MathImageEngine, MathMode as LibMathMode, MermaidEngine as LibMermaidEngine, Severity, TableOfContents,
    Theme,
};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    /// Editable Word equations (OMML), converted in-process. Runs no subprocess.
    Native,
    /// Transparent PNGs rendered with a local TeX install (`latex` + `dvipng`).
    Image,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MermaidEngine {
    /// `merman-cli`: headless Rust renderer, no browser (`cargo install merman-cli`).
    Merman,
    /// `mmdc`: the official Mermaid CLI; drives a headless Chrome.
    Mmdc,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Plain grey code blocks (default).
    None,
    /// Syntax highlighting in the VS Code Light+ colours.
    Light,
    /// Syntax highlighting in the VS Code Dark+ colours.
    Dark,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeBlocks {
    /// One paragraph per source line (default).
    Lines,
    /// One paragraph per block, lines separated by line breaks: easy to select and copy.
    Single,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAccess {
    /// Never read local files; images become `[alt text]`.
    None,
    /// Only files inside the input file's directory (symlinks resolved).
    BaseDir,
    /// Any readable file. Only for Markdown you wrote yourself.
    Any,
}

#[derive(Parser, Debug)]
#[command(
    name = "md2docx",
    version,
    about = "High-fidelity Markdown to DOCX compiler with native Math and Graph preprocessing",
    after_help = "Command templates may use {input} and {output} (and {dpi}); both of the first two are required.\n\
                  Example: --dolphin-command 'dolphin -i {input} -o {output}'\n\n\
                  SECURITY: `--math-mode image` runs a local TeX engine on document content, and diagram \
                  rendering runs external tools. Use `--untrusted` for input you do not control."
)]
pub struct Args {
    #[arg(help = "Path to the input markdown file")]
    pub input: PathBuf,

    #[arg(
        short,
        long,
        help = "Output path for the generated .docx file [default: <input>.docx]"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        default_value_t = MathMode::Native,
        help = "Math rendering strategy (Native OMML or Image fallback)"
    )]
    pub math_mode: MathMode,

    #[arg(
        long,
        value_parser = parse_command,
        help = "Custom CLI command layout for Dolphin graphs (e.g., 'dolphin -i {input} -o {output}')"
    )]
    pub dolphin_command: Option<CommandSpec>,

    #[arg(
        long,
        value_enum,
        default_value_t = MermaidEngine::Merman,
        help = "Renderer for ```mermaid blocks: merman (Rust, no browser) or mmdc (official CLI, drives Chrome)"
    )]
    pub mermaid_engine: MermaidEngine,

    #[arg(
        long,
        value_parser = parse_command,
        conflicts_with = "mermaid_engine",
        help = "Custom CLI command layout for Mermaid graphs, replacing --mermaid-engine (e.g., 'mmdc -i {input} -o {output} -p puppeteer.json')"
    )]
    pub mermaid_command: Option<CommandSpec>,

    #[arg(
        long,
        conflicts_with_all = ["math_mode", "dolphin_command", "mermaid_command", "mermaid_engine", "latex", "dvipng", "image_access"],
        help = "Strictest preset for untrusted Markdown: native math, diagram fences stay code, no local images, tight size limits. Starts no subprocess"
    )]
    pub untrusted: bool,

    #[arg(
        long,
        requires = "untrusted",
        help = "With --untrusted: also render Mermaid diagrams with merman-cli (no browser; install with `cargo install merman-cli`). Starts one tightly bounded process per diagram"
    )]
    pub diagrams: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = ImageAccess::BaseDir,
        help = "Which local images may be embedded"
    )]
    pub image_access: ImageAccess,

    #[arg(long, default_value = "latex", help = "`latex` executable (image math mode only)")]
    pub latex: String,

    #[arg(long, default_value = "dvipng", help = "`dvipng` executable (image math mode only)")]
    pub dvipng: String,

    #[arg(
        long,
        value_name = "SECONDS",
        help = "Kill an external renderer after this long [default: 60; 20 with --untrusted --diagrams]"
    )]
    pub timeout: Option<u64>,

    #[arg(
        long,
        value_name = "0-9",
        default_value_t = 6,
        value_parser = clap::value_parser!(u32).range(0..=9),
        help = "Compression of the .docx: 0 stores it uncompressed, 1 is fastest, 9 smallest"
    )]
    pub compression_level: u32,

    #[arg(
        long,
        help = "Fail instead of writing a document that could not be converted faithfully (a dropped image or link, a diagram left as code, an unknown LaTeX command, ...). Every problem is listed with its line and column"
    )]
    pub strict: bool,

    #[arg(
        long,
        value_name = "FILE.docx",
        help = "Take the styles and theme (fonts, sizes, colours, heading looks) from this .docx; styles it lacks are added"
    )]
    pub reference_doc: Option<PathBuf>,

    #[arg(long, help = "Number headings 1, 1.1, 1.1.1 ...")]
    pub number_headings: bool,

    #[arg(
        long,
        value_name = "MAX_LEVEL",
        num_args = 0..=1,
        default_missing_value = "3",
        value_parser = clap::value_parser!(u8).range(1..=6),
        help = "Start the document with a linked table of contents down to this heading level [default: 3]"
    )]
    pub toc: Option<u8>,

    #[arg(long, help = "Turn bare http://, https:// and www. addresses into links")]
    pub autolink: bool,

    #[arg(long, help = "Read ^x^ as superscript and ~x~ as subscript")]
    pub sup_sub: bool,

    #[arg(
        long,
        value_name = "DIR",
        help = "Create the temporary working directory under DIR instead of the system temp directory (DIR must exist)"
    )]
    pub scratch_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "SECONDS",
        help = "Stop the whole conversion after this long [default: none; 120 with --untrusted]"
    )]
    pub deadline: Option<u64>,

    #[arg(
        long,
        help = "If a mermaid/dolphin block cannot be rendered (tool missing, invalid diagram), keep it as a code block instead of failing"
    )]
    pub keep_failed_diagrams: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = CodeBlocks::Lines,
        help = "Code block layout: one paragraph per line, or the whole block as one paragraph (triple-click selects it)"
    )]
    pub code_blocks: CodeBlocks,

    #[arg(
        long,
        value_enum,
        default_value_t = Highlight::None,
        help = "Syntax-highlight code blocks with the built-in lexers (Rust, Python, JS/TS, Java, C/C++, C#, Go, Kotlin, Swift, Ruby, PHP, SQL, Bash, JSON, YAML, TOML, HTML/XML, CSS, diff)"
    )]
    pub highlight: Highlight,
}

/// Split a command template with POSIX shell rules (via `shlex`) and check it
/// can actually produce an image.
fn parse_command(template: &str) -> Result<CommandSpec, String> {
    let mut words = shlex::split(template)
        .ok_or("unbalanced quotes in command")?
        .into_iter();
    let program = words.next().ok_or("command is empty")?;
    let args: Vec<String> = words.collect();
    let mentions = |p: &str| program.contains(p) || args.iter().any(|a| a.contains(p));
    if !mentions("{input}") || !mentions("{output}") {
        return Err("command must contain both {input} and {output} placeholders".into());
    }
    Ok(CommandSpec::new(program, args))
}

fn config(args: &Args) -> Result<Config, String> {
    let mut cfg = if args.untrusted {
        if args.diagrams {
            Config::untrusted_with_diagrams()
        } else {
            Config::untrusted()
        }
    } else {
        let mut cfg = Config::trusted();
        cfg.math.mode = match args.math_mode {
            MathMode::Native => LibMathMode::NativeOmml,
            MathMode::Image => LibMathMode::ImageFallback,
        };
        cfg.math.engine = MathImageEngine::LatexDvipng {
            latex: args.latex.clone(),
            dvipng: args.dvipng.clone(),
        };
        cfg.image_access = match args.image_access {
            ImageAccess::None => LibImageAccess::None,
            ImageAccess::BaseDir => LibImageAccess::BaseDir,
            ImageAccess::Any => LibImageAccess::Any,
        };
        cfg.graphs.use_engine(match args.mermaid_engine {
            MermaidEngine::Merman => LibMermaidEngine::Merman,
            MermaidEngine::Mmdc => LibMermaidEngine::Mmdc,
        });
        if let Some(spec) = &args.mermaid_command {
            cfg.graphs.mermaid = spec.clone();
        }
        if let Some(spec) = &args.dolphin_command {
            cfg.graphs.dolphin = spec.clone();
        }
        cfg
    };
    cfg.compression_level = args.compression_level;
    cfg.autolink_bare_urls = args.autolink;
    cfg.superscript_subscript = args.sup_sub;
    cfg.document.number_headings = args.number_headings;
    if let Some(level) = args.toc {
        cfg.document.table_of_contents = Some(TableOfContents::new().max_level(level));
    }
    if let Some(path) = &args.reference_doc {
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        cfg = cfg.with_reference_docx(&bytes).map_err(|e| e.to_string())?;
    }
    if args.strict {
        cfg.loss_policy = LossPolicy::Reject;
    }
    if let Some(dir) = &args.scratch_dir {
        cfg.scratch_dir = Some(dir.clone());
    }
    if let Some(seconds) = args.deadline {
        cfg.limits.max_conversion_time = Some(Duration::from_secs(seconds));
    }
    if args.keep_failed_diagrams {
        cfg.graphs.on_failure = DiagramFailure::KeepAsCode;
    }
    cfg.highlighting = match args.highlight {
        Highlight::None => None,
        Highlight::Light => Some(Highlighting::new(Theme::vscode_light())),
        Highlight::Dark => Some(Highlighting::new(Theme::vscode_dark())),
    };
    cfg.document.code_blocks = match args.code_blocks {
        CodeBlocks::Lines => CodeBlockLayout::LinePerParagraph,
        CodeBlocks::Single => CodeBlockLayout::SingleParagraph,
    };
    if let Some(seconds) = args.timeout {
        cfg.tool_timeout = Duration::from_secs(seconds);
    }
    Ok(cfg)
}

/// True when `out` is the same file as `input`, however either is spelled: through `.`/`..`,
/// symbolic links or hard links. A path that does not exist yet cannot be the input.
fn is_same_file(input: &Path, out: &Path) -> bool {
    let Ok(input_real) = std::fs::canonicalize(input) else {
        return false;
    };
    let out_real = std::fs::canonicalize(out).or_else(|_| {
        // Not there yet: its directory (resolved) plus its name.
        let dir = match out.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let name = out.file_name().ok_or_else(|| std::io::Error::other("no file name"))?;
        std::fs::canonicalize(dir).map(|d| d.join(name))
    });
    if out_real.is_ok_and(|o| o == input_real) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(a), Ok(b)) = (std::fs::metadata(input), std::fs::metadata(out)) {
            return a.dev() == b.dev() && a.ino() == b.ino();
        }
    }
    false
}

/// Write `bytes` to `out` through a temporary file in the same directory and a rename, so a
/// failure never leaves a half-written `.docx`, and a symbolic link at `out` is replaced, not
/// followed.
fn write_atomically(out: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(".md2docx-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // What `File::create` would give: 0666 less the process umask.
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut tmp = builder.tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    // Replacing an existing file keeps its permissions (a 0600 document stays 0600).
    if let Ok(existing) = std::fs::metadata(out) {
        if existing.is_file() {
            tmp.as_file().set_permissions(existing.permissions())?;
        }
    }
    tmp.persist(out).map_err(|e| e.error)?;
    Ok(())
}

/// `file:line:column: warning[code]: message`, the shape editors and CI understand.
fn describe(input: &Path, d: &silvercity_oss::Diagnostic) -> String {
    let place = d
        .span
        .as_ref()
        .map_or_else(String::new, |s| format!(":{}:{}", s.line, s.column));
    format!("{}{place}: {}[{}]: {}", input.display(), d.severity, d.code, d.message)
}

fn run(args: &Args) -> Result<PathBuf, String> {
    let out = args.output.clone().unwrap_or_else(|| args.input.with_extension("docx"));
    if is_same_file(&args.input, &out) {
        return Err(format!("refusing to overwrite the input file {}", args.input.display()));
    }
    let conversion = match convert_file(&args.input, &config(args)?) {
        Ok(c) => c,
        Err(silvercity_oss::CrateError::LossyContent { diagnostics }) => {
            for d in &diagnostics {
                eprintln!("{}", describe(&args.input, d));
            }
            return Err("the document cannot be converted faithfully (--strict); nothing was written".into());
        }
        Err(e) => return Err(e.to_string()),
    };
    for d in &conversion.diagnostics {
        if d.severity >= Severity::Warning {
            eprintln!("{}", describe(&args.input, d));
        }
    }
    write_atomically(&out, &conversion.docx).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(out)
}

/// Longest per-tool timeout `--untrusted --diagrams` accepts (the preset's own).
const UNTRUSTED_MAX_TIMEOUT_SECS: u64 = 20;

fn main() -> ExitCode {
    let args = Args::parse();
    if args.untrusted && args.diagrams && args.timeout.is_some_and(|t| t > UNTRUSTED_MAX_TIMEOUT_SECS) {
        Args::command()
            .error(
                clap::error::ErrorKind::ValueValidation,
                format!("--timeout may not exceed {UNTRUSTED_MAX_TIMEOUT_SECS} seconds with --untrusted --diagrams"),
            )
            .exit();
    }
    match run(&args) {
        Ok(out) => {
            eprintln!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_templates_are_split_like_a_shell() {
        let c = parse_command(r#"dolphin --title "my graph" -i {input} -o {output}"#).unwrap();
        assert_eq!(c.program, "dolphin");
        assert_eq!(c.args, ["--title", "my graph", "-i", "{input}", "-o", "{output}"]);
    }

    #[test]
    fn bad_command_templates_are_rejected() {
        assert!(parse_command("").is_err());
        assert!(parse_command("dolphin 'unterminated -i {input} -o {output}").is_err());
        assert!(parse_command("dolphin -i {input}").is_err());
        assert!(parse_command("dolphin -o {output}").is_err());
    }

    #[test]
    fn clap_definition_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }

    #[test]
    fn untrusted_preset_is_strict_and_exclusive() {
        let a = Args::try_parse_from(["md2docx", "in.md", "--untrusted"]).unwrap();
        let c = config(&a).unwrap();
        assert_eq!(c.math.mode, LibMathMode::NativeOmml);
        assert!(!c.graphs.enabled);
        assert_eq!(c.image_access, LibImageAccess::None);
        // It cannot be combined with switches that would loosen it.
        for extra in [
            ["--math-mode", "image"],
            ["--image-access", "any"],
            ["--dolphin-command", "d -i {input} -o {output}"],
            ["--latex", "xelatex"],
        ] {
            assert!(
                Args::try_parse_from(["md2docx", "in.md", "--untrusted", extra[0], extra[1]]).is_err(),
                "{extra:?}"
            );
        }
        assert!(Args::try_parse_from(["md2docx", "in.md", "--untrusted", "--mermaid-engine", "mmdc"]).is_err());
        // Defaults for the normal mode keep images inside the input's directory.
        let d = Args::try_parse_from(["md2docx", "in.md"]).unwrap();
        assert_eq!(config(&d).unwrap().image_access, LibImageAccess::BaseDir);
    }

    #[test]
    fn mermaid_engine_defaults_to_merman_and_can_switch_to_mmdc() {
        let d = Args::try_parse_from(["md2docx", "in.md"]).unwrap();
        assert_eq!(config(&d).unwrap().graphs.mermaid.program, "merman-cli");
        let m = Args::try_parse_from(["md2docx", "in.md", "--mermaid-engine", "mmdc"]).unwrap();
        assert_eq!(config(&m).unwrap().graphs.mermaid.program, "mmdc");
        // An explicit command wins, and cannot be combined with the engine switch.
        let c = Args::try_parse_from([
            "md2docx",
            "in.md",
            "--mermaid-command",
            "my-mmdc -i {input} -o {output}",
        ])
        .unwrap();
        assert_eq!(config(&c).unwrap().graphs.mermaid.program, "my-mmdc");
        assert!(Args::try_parse_from([
            "md2docx",
            "in.md",
            "--mermaid-engine",
            "mmdc",
            "--mermaid-command",
            "x -i {input} -o {output}"
        ])
        .is_err());
    }

    #[test]
    fn flags_map_onto_the_library_config() {
        let a = Args::try_parse_from([
            "md2docx",
            "in.md",
            "--math-mode",
            "image",
            "--dolphin-command",
            "d -i {input} -o {output}",
            "--timeout",
            "5",
            "--latex",
            "xelatex",
        ])
        .unwrap();
        let c = config(&a).unwrap();
        assert_eq!(c.math.mode, LibMathMode::ImageFallback);
        assert_eq!(c.graphs.dolphin.program, "d");
        assert_eq!(c.tool_timeout, Duration::from_secs(5));
        assert!(matches!(&c.math.engine, MathImageEngine::LatexDvipng { latex, .. } if latex == "xelatex"));
        // Default is the safe, tool-free mode.
        let d = Args::try_parse_from(["md2docx", "in.md"]).unwrap();
        assert_eq!(config(&d).unwrap().math.mode, LibMathMode::NativeOmml);
    }
}
