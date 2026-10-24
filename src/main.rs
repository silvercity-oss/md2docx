//! `reqmech`: command-line front end for the Markdown -> .docx converter.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use reqmech_companion::{markdown_file_to_docx, markdown_to_docx, Config, CrateError, MathImageEngine, MathMode};

const USAGE: &str = "\
Usage: reqmech <input.md | -> [-o output.docx] [options]

Options:
  -o, --output <file>   Output path (default: input with .docx extension; required for stdin)
      --math <mode>     `image` (default, needs latex+dvipng) or `omml` (native, editable)
      --mmdc <program>  Mermaid CLI to use (default: mmdc)
      --dolphin <prog>  Dolphin CLI to use (default: dolphin)
      --latex <prog>    latex binary for image math (default: latex)
      --dvipng <prog>   dvipng binary for image math (default: dvipng)
  -h, --help            Show this help
";

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut input: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut cfg = Config::default();
    let (mut latex, mut dvipng) = ("latex".to_string(), "dvipng".to_string());

    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-o" | "--output" => output = Some(PathBuf::from(value("--output")?)),
            "--math" => {
                cfg.math.mode = match value("--math")?.as_str() {
                    "image" => MathMode::ImageFallback,
                    "omml" => MathMode::NativeOmml,
                    other => return Err(format!("unknown math mode `{other}` (expected `image` or `omml`)")),
                }
            }
            "--mmdc" => cfg.graphs.mermaid.program = value("--mmdc")?,
            "--dolphin" => cfg.graphs.dolphin.program = value("--dolphin")?,
            "--latex" => latex = value("--latex")?,
            "--dvipng" => dvipng = value("--dvipng")?,
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option `{s}`\n\n{USAGE}")),
            _ if input.is_none() => input = Some(arg),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    cfg.math.engine = MathImageEngine::LatexDvipng { latex, dvipng };

    let input = input.ok_or_else(|| format!("missing input file\n\n{USAGE}"))?;
    let describe = |e: CrateError| e.to_string();
    let (bytes, out_path) = if input == "-" {
        let out = output.ok_or("`-o` is required when reading from stdin")?;
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        (markdown_to_docx(&text, &cfg).map_err(describe)?, out)
    } else {
        let path = PathBuf::from(&input);
        let out = output.unwrap_or_else(|| path.with_extension("docx"));
        (markdown_file_to_docx(&path, &cfg).map_err(describe)?, out)
    };
    std::fs::write(&out_path, bytes).map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
    eprintln!("wrote {}", out_path.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
