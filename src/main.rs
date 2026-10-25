//! `md2docx`: command-line front end for the Markdown -> .docx converter.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use silvercity_oss::{
    markdown_file_to_docx, CodeBlockLayout, CommandSpec, Config, ImageAccess as LibImageAccess, MathImageEngine,
    MathMode as LibMathMode, MermaidEngine as LibMermaidEngine,
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
        value_enum,
        default_value_t = CodeBlocks::Lines,
        help = "Code block layout: one paragraph per line, or the whole block as one paragraph (triple-click selects it)"
    )]
    pub code_blocks: CodeBlocks,
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
    Ok(CommandSpec { program, args })
}

fn config(args: &Args) -> Config {
    let mut cfg = if args.untrusted {
        if args.diagrams {
            Config::untrusted_with_diagrams()
        } else {
            Config::untrusted()
        }
    } else {
        let mut cfg = Config::default();
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
    cfg.document.code_blocks = match args.code_blocks {
        CodeBlocks::Lines => CodeBlockLayout::LinePerParagraph,
        CodeBlocks::Single => CodeBlockLayout::SingleParagraph,
    };
    if let Some(seconds) = args.timeout {
        cfg.tool_timeout = Duration::from_secs(seconds);
    }
    cfg
}

fn run(args: &Args) -> Result<PathBuf, String> {
    let out = args.output.clone().unwrap_or_else(|| args.input.with_extension("docx"));
    if out == args.input {
        return Err(format!("refusing to overwrite the input file {}", out.display()));
    }
    let bytes = markdown_file_to_docx(&args.input, &config(args)).map_err(|e| e.to_string())?;
    std::fs::write(&out, bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    Ok(out)
}

fn main() -> ExitCode {
    let args = Args::parse();
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
        let c = config(&a);
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
        assert_eq!(config(&d).image_access, LibImageAccess::BaseDir);
    }

    #[test]
    fn mermaid_engine_defaults_to_merman_and_can_switch_to_mmdc() {
        let d = Args::try_parse_from(["md2docx", "in.md"]).unwrap();
        assert_eq!(config(&d).graphs.mermaid.program, "merman-cli");
        let m = Args::try_parse_from(["md2docx", "in.md", "--mermaid-engine", "mmdc"]).unwrap();
        assert_eq!(config(&m).graphs.mermaid.program, "mmdc");
        // An explicit command wins, and cannot be combined with the engine switch.
        let c = Args::try_parse_from([
            "md2docx",
            "in.md",
            "--mermaid-command",
            "my-mmdc -i {input} -o {output}",
        ])
        .unwrap();
        assert_eq!(config(&c).graphs.mermaid.program, "my-mmdc");
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
        let c = config(&a);
        assert_eq!(c.math.mode, LibMathMode::ImageFallback);
        assert_eq!(c.graphs.dolphin.program, "d");
        assert_eq!(c.tool_timeout, Duration::from_secs(5));
        assert!(matches!(&c.math.engine, MathImageEngine::LatexDvipng { latex, .. } if latex == "xelatex"));
        // Default is the safe, tool-free mode.
        let d = Args::try_parse_from(["md2docx", "in.md"]).unwrap();
        assert_eq!(config(&d).math.mode, LibMathMode::NativeOmml);
    }
}
