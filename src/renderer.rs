//! Stage 1 helpers: run external CLIs and turn graph/math sources into PNGs.
//!
//! Every file lives inside a [`Workspace`], a `tempfile::TempDir` owned by the
//! conversion, so nothing outlives the call even on error paths.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::config::{CommandSpec, Config, MathImageEngine};
use crate::error::{CrateError, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Kind of text-based diagram handled by the pre-processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphKind {
    Mermaid,
    Dolphin,
}

impl GraphKind {
    pub fn from_info(lang: &str) -> Option<Self> {
        match lang.split_whitespace().next()?.to_ascii_lowercase().as_str() {
            "mermaid" => Some(GraphKind::Mermaid),
            "dolphin" => Some(GraphKind::Dolphin),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            GraphKind::Mermaid => "mermaid",
            GraphKind::Dolphin => "dolphin",
        }
    }
}

/// RAII scratch directory. Dropping it deletes every rendered asset.
#[derive(Debug)]
pub struct Workspace {
    dir: TempDir,
}

impl Workspace {
    pub fn new() -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("reqmech-")
            .tempdir()
            .map_err(|e| CrateError::io(std::env::temp_dir(), e))?;
        Ok(Workspace { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Deterministic, collision-resistant file stem: `<prefix>_<16 hex of sha256>`.
pub fn asset_stem(prefix: &str, parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let digest = h.finalize();
    let mut stem = String::with_capacity(prefix.len() + 17);
    stem.push_str(prefix);
    stem.push('_');
    for b in digest.iter().take(8) {
        stem.push_str(&format!("{b:02x}"));
    }
    stem
}

fn substitute(arg: &str, input: &Path, output: &Path, dpi: u32, display: bool) -> String {
    arg.replace("{input}", &input.to_string_lossy())
        .replace("{output}", &output.to_string_lossy())
        .replace("{dpi}", &dpi.to_string())
        .replace("{display}", if display { "1" } else { "0" })
}

/// `ETXTBSY`: the executable is still open for writing somewhere (typically a
/// tool that was written or installed a moment ago while another thread forked
/// and briefly inherited the descriptor). It clears within milliseconds.
#[cfg(unix)]
fn is_text_file_busy(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(26)
}

#[cfg(not(unix))]
fn is_text_file_busy(_: &std::io::Error) -> bool {
    false
}

/// Run `program args...` in `cwd`, killing it after `timeout`. stdout and
/// stderr are captured into an anonymous temp file (no pipe deadlocks) and
/// surfaced in the error on failure.
pub fn run_tool(program: &str, args: &[String], cwd: &Path, timeout: Duration) -> Result<()> {
    let tool_err = |detail: String| CrateError::ExternalToolFailed {
        program: program.to_string(),
        detail,
    };

    let mut log = tempfile::tempfile_in(cwd).map_err(|e| CrateError::io(cwd, e))?;

    let mut attempt = 0u32;
    let mut child = loop {
        // Stdio handles are consumed by `spawn`, so clone them per attempt.
        let (out, err) = (
            log.try_clone().map_err(|e| CrateError::io(cwd, e))?,
            log.try_clone().map_err(|e| CrateError::io(cwd, e))?,
        );
        let spawned = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .spawn();
        match spawned {
            Ok(child) => break child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CrateError::MissingExternalCLI {
                    program: program.to_string(),
                });
            }
            Err(e) if is_text_file_busy(&e) && attempt < 5 => {
                attempt += 1;
                sleep(Duration::from_millis(20 * u64::from(attempt)));
            }
            Err(e) => return Err(tool_err(format!("could not start: {e}"))),
        }
    };

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(tool_err(format!("timed out after {}s", timeout.as_secs())));
            }
            Ok(None) => sleep(Duration::from_millis(15)),
            Err(e) => return Err(tool_err(format!("wait failed: {e}"))),
        }
    };

    if status.success() {
        return Ok(());
    }
    let mut output = String::new();
    let _ = log.seek(SeekFrom::Start(0));
    let _ = log.take(8 * 1024).read_to_string(&mut output);
    Err(tool_err(format!("exited with {status}: {}", output.trim())))
}

fn check_png(program: &str, path: &Path) -> Result<()> {
    let mut head = [0u8; 8];
    let ok = fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && head == PNG_SIGNATURE;
    if ok {
        Ok(())
    } else {
        Err(CrateError::ExternalToolFailed {
            program: program.to_string(),
            detail: format!("did not produce a PNG at {}", path.display()),
        })
    }
}

fn run_spec(
    spec: &CommandSpec,
    ws: &Workspace,
    input: &Path,
    output: &Path,
    dpi: u32,
    display: bool,
    cfg: &Config,
) -> Result<()> {
    let args: Vec<String> = spec
        .args
        .iter()
        .map(|a| substitute(a, input, output, dpi, display))
        .collect();
    run_tool(&spec.program, &args, ws.path(), cfg.tool_timeout)?;
    check_png(&spec.program, output)
}

/// Render a Mermaid/Dolphin source to `<workspace>/graph_<hash>.png`.
pub fn render_graph(ws: &Workspace, kind: GraphKind, source: &str, cfg: &Config) -> Result<PathBuf> {
    let stem = asset_stem("graph", &[kind.name().as_bytes(), source.as_bytes()]);
    let output = ws.path().join(format!("{stem}.png"));
    if output.exists() {
        return Ok(output);
    }
    let input = ws.path().join(format!(
        "{stem}.{}",
        if kind == GraphKind::Mermaid { "mmd" } else { "dolphin" }
    ));
    fs::write(&input, source).map_err(|e| CrateError::io(&input, e))?;
    let spec = match kind {
        GraphKind::Mermaid => &cfg.graphs.mermaid,
        GraphKind::Dolphin => &cfg.graphs.dolphin,
    };
    run_spec(spec, ws, &input, &output, cfg.graphs.dpi, false, cfg)?;
    Ok(output)
}

/// LaTeX document wrapping a single formula, for the `latex` + `dvipng` engine.
fn latex_document(formula: &str, display: bool) -> String {
    let body = if display {
        format!("\\[\n{formula}\n\\]")
    } else {
        format!("${formula}$")
    };
    format!(
        "\\documentclass[12pt]{{article}}\n\\usepackage{{amsmath,amssymb}}\n\\pagestyle{{empty}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"
    )
}

/// Render a LaTeX formula to a transparent PNG `<workspace>/math_<hash>.png`.
pub fn render_math_image(ws: &Workspace, latex: &str, display: bool, cfg: &Config) -> Result<PathBuf> {
    let dpi = cfg.math.dpi;
    let tag: &[u8] = if display { b"display" } else { b"inline" };
    let stem = asset_stem("math", &[tag, latex.as_bytes(), &dpi.to_le_bytes()]);
    let output = ws.path().join(format!("{stem}.png"));
    if output.exists() {
        return Ok(output);
    }
    match &cfg.math.engine {
        MathImageEngine::LatexDvipng {
            latex: latex_bin,
            dvipng,
        } => {
            let tex = ws.path().join(format!("{stem}.tex"));
            fs::write(&tex, latex_document(latex, display)).map_err(|e| CrateError::io(&tex, e))?;
            let tex_arg = tex.to_string_lossy().into_owned();
            run_tool(
                latex_bin,
                &[
                    "-interaction=nonstopmode".into(),
                    "-halt-on-error".into(),
                    "-no-shell-escape".into(),
                    tex_arg,
                ],
                ws.path(),
                cfg.tool_timeout,
            )?;
            let dvi = ws.path().join(format!("{stem}.dvi"));
            run_tool(
                dvipng,
                &[
                    "-T".into(),
                    "tight".into(),
                    "-bg".into(),
                    "Transparent".into(),
                    "-D".into(),
                    dpi.to_string(),
                    "-o".into(),
                    output.to_string_lossy().into_owned(),
                    dvi.to_string_lossy().into_owned(),
                ],
                ws.path(),
                cfg.tool_timeout,
            )?;
            check_png(dvipng, &output)?;
        }
        MathImageEngine::Command(spec) => {
            let input = ws.path().join(format!("{stem}.tex"));
            fs::write(&input, latex).map_err(|e| CrateError::io(&input, e))?;
            run_spec(spec, ws, &input, &output, dpi, display, cfg)?;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_stems_are_deterministic_and_distinct() {
        let a = asset_stem("graph", &[b"mermaid", b"A-->B"]);
        assert_eq!(a, asset_stem("graph", &[b"mermaid", b"A-->B"]));
        assert_ne!(a, asset_stem("graph", &[b"mermaid", b"A-->C"]));
        assert!(a.starts_with("graph_") && a.len() == "graph_".len() + 16);
        // Part boundaries matter: ("ab","c") != ("a","bc").
        assert_ne!(asset_stem("x", &[b"ab", b"c"]), asset_stem("x", &[b"a", b"bc"]));
    }

    #[test]
    fn graph_languages_are_recognised_case_insensitively() {
        assert_eq!(GraphKind::from_info("Mermaid"), Some(GraphKind::Mermaid));
        assert_eq!(GraphKind::from_info("dolphin {x=1}"), Some(GraphKind::Dolphin));
        assert_eq!(GraphKind::from_info("rust"), None);
        assert_eq!(GraphKind::from_info(""), None);
    }

    #[test]
    fn workspace_is_removed_on_drop() {
        let ws = Workspace::new().unwrap();
        let path = ws.path().to_path_buf();
        assert!(path.exists());
        drop(ws);
        assert!(!path.exists());
    }
}
