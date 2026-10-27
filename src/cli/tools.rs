//! The modes that print facts and stop: `--extract-reference`, `--capabilities`, `--list-codes`
//! and `--print-config`. None of them converts Markdown.

use std::io::Write;
use std::path::{Path, PathBuf};

use md2docx::{BuiltinHighlighter, Code, Config, StyleSnapshot, STYLE_SCHEMA};
use serde_json::{json, Value};

use super::args::{Args, Format};
use super::exit::{CliError, Exit};
use super::run::write_atomically;
use super::settings;

/// Dispatch to the mode that was asked for, if any. `None` means: convert as usual.
pub fn dispatch(args: &Args) -> Option<u8> {
    let result = if args.capabilities {
        Some(print(&capabilities(args)))
    } else if args.list_codes {
        Some(print(&Code::catalog_json()))
    } else if args.print_config {
        Some(settings::config(args).and_then(|cfg| print(&cfg.settings_json())))
    } else {
        args.extract_reference
            .as_ref()
            .map(|docx| extract_reference(args, docx))
    };
    result.map(|r| match r {
        Ok(()) => Exit::Ok.code(),
        Err(e) => {
            if args.format == Format::Json {
                say!(
                    "{}",
                    json!({"schema": 1, "status": "error", "exit_code": e.exit.code(),
                           "error": {"kind": e.kind, "message": e.message}})
                );
            } else {
                say!("error: {}", e.message);
            }
            e.exit.code()
        }
    })
}

fn print(text: &str) -> Result<(), CliError> {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| CliError::io(format!("cannot write to standard output: {e}")))
}

/// `--extract-reference FILE.docx`: the styles a conversion would use, as JSON.
fn extract_reference(args: &Args, docx: &Path) -> Result<(), CliError> {
    // `config` reads `docx` as the reference document, so the snapshot is "what --reference-doc
    // would use", with any look options (--body-font, ...) applied on top.
    let cfg = settings::config(args)?;
    let source = docx
        .file_name()
        .map_or_else(|| docx.display().to_string(), |n| n.to_string_lossy().into_owned());
    let json = StyleSnapshot::from_config(&cfg)?.with_source(source).to_json();
    let to_stdout = args.output.as_deref().is_some_and(|o| o.as_os_str() == "-");
    if to_stdout {
        return print(&json);
    }
    let out: PathBuf = args.output.clone().unwrap_or_else(|| docx.with_extension("json"));
    if out.as_path() == docx {
        return Err(CliError::usage(format!(
            "--extract-reference would overwrite its own input {}; give -o a .json path",
            docx.display()
        )));
    }
    write_atomically(&out, json.as_bytes())
        .map_err(|e| CliError::io(format!("cannot write {}: {e}", out.display())))?;
    if !args.quiet && args.format == Format::Text {
        say!("wrote {}", out.display());
    }
    Ok(())
}

/// Where `program` is on `PATH`, if anywhere.
fn which(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&paths).find_map(|dir| {
        extensions.iter().find_map(|ext| {
            let candidate = dir.join(format!("{program}{ext}"));
            candidate.is_file().then_some(candidate)
        })
    })
}

fn tool(program: &str) -> Value {
    match which(program) {
        Some(path) => json!({"found": true, "path": path.display().to_string()}),
        None => json!({"found": false, "path": null}),
    }
}

fn preset(cfg: &Config) -> Value {
    serde_json::from_str(&cfg.settings_json()).unwrap_or(Value::Null)
}

/// `--capabilities`: what this build can do, for a program deciding how to call it.
fn capabilities(args: &Args) -> String {
    let _ = args;
    let mermaid = cfg!(feature = "mermaid");
    let exit_codes = json!([
        {"code": 0, "name": "ok", "meaning": "converted (with --check: convertible); warnings may have been reported"},
        {"code": 1, "name": "failure", "meaning": "anything else, including a bug or a cancelled conversion"},
        {"code": 2, "name": "usage", "meaning": "the command line or configuration is wrong; nothing was converted"},
        {"code": 3, "name": "io", "meaning": "a file could not be read or written"},
        {"code": 4, "name": "rejected", "meaning": "the document is malformed, over a limit or forbidden by the security policy; retrying gives the same answer"},
        {"code": 5, "name": "lossy", "meaning": "--strict: the document could not be converted without loss, so nothing was written"},
        {"code": 6, "name": "tool", "meaning": "an external program (latex, dvipng, mmdc, merman-cli, a custom renderer) is missing or failed"},
        {"code": 7, "name": "timeout", "meaning": "--deadline passed; a retry with more time may succeed"},
    ]);
    let value = json!({
        "schema": 1,
        "name": "md2docx-cli",
        "version": env!("CARGO_PKG_VERSION"),
        "features": {"cli": true, "mermaid_in_process": mermaid},
        "input": {
            "markdown": "CommonMark + GitHub tables, task lists, footnotes, strikethrough, alerts, front matter, $math$, ```mermaid fences, a safe subset of raw HTML",
            "stdin": true,
            "images": ["png", "jpeg", "gif"],
        },
        "output": {"format": "docx", "stdout": true, "check_mode": true, "report_formats": ["text", "json"]},
        "report_schema": 1,
        "style_schema": STYLE_SCHEMA,
        "exit_codes": exit_codes,
        "diagnostic_codes": Code::ALL.len(),
        "options": {
            "math_mode": ["native", "image"],
            "mermaid_engine": if mermaid { json!(["merman", "mmdc", "inprocess"]) } else { json!(["merman", "mmdc"]) },
            "image_access": ["none", "base-dir", "any"],
            "image_encoding": ["png", "original"],
            "code_blocks": ["lines", "single"],
            "highlight": ["none", "light", "dark"],
            "highlight_languages": BuiltinHighlighter::LANGUAGES,
            "page_size": ["a4", "letter", "legal", "WIDTHxHEIGHT with a unit, e.g. 148x210mm or 8x10in"],
            "allow_links": ["none", "local", "all"],
            "link_style": ["rebased", "absolute"],
            "markdown_links": ["keep", "docx", "follow"],
            "link_layout": ["mirror", "flat"],
            "completions": ["bash", "elvish", "fish", "powershell", "zsh"],
        },
        "environment_variables": [
            "MD2DOCX_FORMAT", "MD2DOCX_QUIET", "MD2DOCX_STRICT", "MD2DOCX_UNTRUSTED", "MD2DOCX_BASE_DIR",
            "MD2DOCX_TIMEOUT", "MD2DOCX_SCRATCH_DIR", "MD2DOCX_DEADLINE", "MD2DOCX_MAX_INPUT_BYTES",
            "MD2DOCX_MAX_OUTPUT_BYTES", "MD2DOCX_MAX_UNCOMPRESSED_BYTES", "MD2DOCX_MAX_ELEMENTS",
            "MD2DOCX_MAX_IMAGE_BYTES", "MD2DOCX_MAX_IMAGES", "SOURCE_DATE_EPOCH"
        ],
        "presets": {
            "trusted": preset(&Config::trusted()),
            "untrusted": preset(&Config::untrusted()),
        },
        "external_tools": {
            "latex": tool("latex"),
            "dvipng": tool("dvipng"),
            "merman-cli": tool("merman-cli"),
            "mmdc": tool("mmdc"),
        },
    });
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}
