//! `md2docx`: command-line front end for the Markdown -> .docx converter.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use reqmech_companion::{markdown_file_to_docx, CommandSpec, Config, MathImageEngine, MathMode as LibMathMode};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    /// Editable Word equations (OMML), converted in-process. Runs no subprocess.
    Native,
    /// Transparent PNGs rendered with a local TeX install (`latex` + `dvipng`).
    Image,
}

#[derive(Parser, Debug)]
#[command(
    name = "md2docx",
    version,
    about = "High-fidelity Markdown to DOCX compiler with native Math and Graph preprocessing",
    after_help = "Command templates may use {input} and {output} (and {dpi}); both of the first two are required.\n\
                  Example: --dolphin-command 'dolphin -i {input} -o {output}'\n\n\
                  SECURITY: `--math-mode image` runs a local TeX engine on document content. Use the default \
                  `native` mode for untrusted input."
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
        value_parser = parse_command,
        help = "Custom CLI command layout for Mermaid graphs [default: mmdc -i {input} -o {output} -s 3 -b transparent]"
    )]
    pub mermaid_command: Option<CommandSpec>,

    #[arg(long, default_value = "latex", help = "`latex` executable (image math mode only)")]
    pub latex: String,

    #[arg(long, default_value = "dvipng", help = "`dvipng` executable (image math mode only)")]
    pub dvipng: String,

    #[arg(
        long,
        default_value_t = 60,
        value_name = "SECONDS",
        help = "Kill an external renderer after this long"
    )]
    pub timeout: u64,
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
    let mut cfg = Config::default();
    cfg.math.mode = match args.math_mode {
        MathMode::Native => LibMathMode::NativeOmml,
        MathMode::Image => LibMathMode::ImageFallback,
    };
    cfg.math.engine = MathImageEngine::LatexDvipng {
        latex: args.latex.clone(),
        dvipng: args.dvipng.clone(),
    };
    if let Some(spec) = &args.mermaid_command {
        cfg.graphs.mermaid = spec.clone();
    }
    if let Some(spec) = &args.dolphin_command {
        cfg.graphs.dolphin = spec.clone();
    }
    cfg.tool_timeout = Duration::from_secs(args.timeout);
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
