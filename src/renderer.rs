//! Stage 1 helpers: run external CLIs and turn graph/math sources into PNGs.
//!
//! Every file lives inside a `Workspace`, a `tempfile::TempDir` owned by the
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
    /// When the conversion's total external-tool time is used up.
    deadline: Option<Instant>,
}

impl Workspace {
    #[cfg(test)]
    pub fn new() -> Result<Self> {
        Self::with_budget(None)
    }

    /// A workspace whose tools may run for `budget` in total (`None`: only the
    /// per-tool timeout applies).
    pub fn with_budget(budget: Option<Duration>) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("silvercity-")
            .tempdir()
            .map_err(|e| CrateError::io(std::env::temp_dir(), e))?;
        Ok(Workspace {
            dir,
            deadline: budget.map(|b| Instant::now() + b),
        })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Fail when the conversion's total time for external work is used up.
    pub fn check_budget(&self, what: &str) -> Result<()> {
        match self.deadline {
            Some(d) if Instant::now() >= d => Err(CrateError::LimitExceeded(format!(
                "the time budget for external work ran out before {what} finished"
            ))),
            _ => Ok(()),
        }
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

/// Environment that puts TeX in *paranoid* file-access mode: it may only read
/// and write inside its working directory (no absolute paths, no `..`, no
/// dotfiles) and may never run programs. Complements `-no-shell-escape`, which
/// alone does not stop `\input{/etc/passwd}` from reading host files.
pub const TEX_ENV: &[(&str, &str)] = &[("openin_any", "p"), ("openout_any", "p"), ("shell_escape", "f")];

/// Console output (stdout + stderr) a tool may produce before it is killed.
const MAX_TOOL_LOG_BYTES: u64 = 1024 * 1024;

/// Extra constraints for one tool invocation.
#[derive(Debug, Clone, Copy)]
pub struct ToolGuard<'a> {
    /// Environment variables to set (on top of the inherited environment).
    pub env: &'a [(&'a str, &'a str)],
    /// Kill the tool if `cwd` grows beyond this many bytes.
    pub max_dir_bytes: u64,
    /// Kill the tool (and refuse to start new ones) after this instant: the
    /// conversion's total time budget for external tools.
    pub overall_deadline: Option<Instant>,
    /// Run with a scrubbed environment: only `PATH` (and the few variables a
    /// process needs on Windows) are inherited, and `HOME`/`TMPDIR` point at this
    /// directory, so nothing from the host's environment reaches the tool.
    pub scrub_env_to: Option<&'a Path>,
}

/// Most directory entries `dir_size` will look at.
const MAX_DIR_ENTRIES: usize = 20_000;

/// Total size of the files under `dir`, without following symbolic links. A tree with more
/// than `MAX_DIR_ENTRIES` entries counts as unboundedly large.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_DIR_ENTRIES {
                return u64::MAX;
            }
            // `DirEntry::metadata` does not follow symlinks.
            match entry.metadata() {
                Ok(m) if m.is_dir() => pending.push(entry.path()),
                Ok(m) => total = total.saturating_add(m.len()),
                Err(_) => {}
            }
        }
    }
    total
}

/// Kill the tool and everything it started (Chrome behind `mmdc`, for example).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        // The child leads its own process group (see `spawn`), so this reaches its descendants.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn budget_exhausted() -> CrateError {
    CrateError::LimitExceeded("the document's total time budget for external tools was used up".into())
}

/// Run `program args...` in `cwd`.
///
/// No shell is involved: arguments are passed as an argv list. The tool is
/// killed (with its descendants, on Unix) after `timeout`, if it prints more
/// than 1 MiB, or if its working directory outgrows `guard.max_dir_bytes`.
/// stdout and stderr go to an anonymous temp file (no pipe deadlocks) and are
/// surfaced in the error on failure.
pub fn run_tool(program: &str, args: &[String], cwd: &Path, timeout: Duration, guard: &ToolGuard<'_>) -> Result<()> {
    let tool_err = |detail: String| CrateError::ExternalToolFailed {
        program: program.to_string(),
        detail,
    };

    if guard.overall_deadline.is_some_and(|d| Instant::now() >= d) {
        return Err(budget_exhausted());
    }
    let mut log = tempfile::tempfile_in(cwd).map_err(|e| CrateError::io(cwd, e))?;

    let mut attempt = 0u32;
    let mut child = loop {
        // Stdio handles are consumed by `spawn`, so clone them per attempt.
        let (out, err) = (
            log.try_clone().map_err(|e| CrateError::io(cwd, e))?,
            log.try_clone().map_err(|e| CrateError::io(cwd, e))?,
        );
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err));
        if let Some(home) = guard.scrub_env_to {
            cmd.env_clear();
            for k in [
                "PATH",
                "PATHEXT",
                "SystemRoot",
                "SYSTEMROOT",
                "WINDIR",
                "LANG",
                "LC_ALL",
            ] {
                if let Some(v) = std::env::var_os(k) {
                    cmd.env(k, v);
                }
            }
            for k in ["HOME", "TMPDIR", "TEMP", "TMP"] {
                cmd.env(k, home);
            }
        }
        for (k, v) in guard.env {
            cmd.env(k, v);
        }
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        match cmd.spawn() {
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

    let own_deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if guard.overall_deadline.is_some_and(|d| now >= d) {
                    kill_tree(&mut child);
                    return Err(budget_exhausted());
                }
                if now >= own_deadline {
                    kill_tree(&mut child);
                    return Err(tool_err(format!("timed out after {}s", timeout.as_secs())));
                }
                if log.metadata().map(|m| m.len()).unwrap_or(0) > MAX_TOOL_LOG_BYTES {
                    kill_tree(&mut child);
                    return Err(tool_err("killed: more than 1 MiB of console output".into()));
                }
                if dir_size(cwd) > guard.max_dir_bytes {
                    kill_tree(&mut child);
                    return Err(CrateError::LimitExceeded(format!(
                        "`{program}` wrote more than {} bytes of files",
                        guard.max_dir_bytes
                    )));
                }
                sleep(Duration::from_millis(10));
            }
            Err(e) => {
                kill_tree(&mut child);
                return Err(tool_err(format!("wait failed: {e}")));
            }
        }
    };

    // Polling can miss a tool that writes faster than the poll interval (a fast
    // disk writes hundreds of MB in a few milliseconds), so enforce the cap once more
    // now that it has exited. The kill above only limits how far it can overshoot.
    if dir_size(cwd) > guard.max_dir_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "`{program}` wrote more than {} bytes of files",
            guard.max_dir_bytes
        )));
    }
    // Whatever the tool's own outcome, helpers it started must not outlive it.
    #[cfg(unix)]
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    if status.success() {
        return Ok(());
    }
    let mut output = String::new();
    let _ = log.seek(SeekFrom::Start(0));
    let _ = log.take(8 * 1024).read_to_string(&mut output);
    Err(tool_err(format!("exited with {status}: {}", output.trim())))
}

/// The output must be a PNG no larger than `max_bytes`.
fn check_png(program: &str, path: &Path, max_bytes: u64) -> Result<()> {
    let bad = |detail: String| CrateError::ExternalToolFailed {
        program: program.to_string(),
        detail,
    };
    // Not `metadata`: a symbolic link left in place of the output could point at any image.
    let meta = fs::symlink_metadata(path).map_err(|_| bad(format!("did not produce {}", path.display())))?;
    if !meta.is_file() {
        return Err(bad(format!("{} is not a regular file", path.display())));
    }
    let len = meta.len();
    if len > max_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "`{program}` produced a {len} byte image (limit {max_bytes})"
        )));
    }
    let mut head = [0u8; 8];
    let ok = fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && head == PNG_SIGNATURE;
    if ok {
        Ok(())
    } else {
        Err(bad(format!("did not produce a PNG at {}", path.display())))
    }
}

fn guard<'a>(ws: &Workspace, cfg: &Config, env: &'a [(&'a str, &'a str)]) -> ToolGuard<'a> {
    ToolGuard {
        env,
        max_dir_bytes: cfg.limits.max_workspace_bytes,
        overall_deadline: ws.deadline,
        scrub_env_to: None,
    }
}

/// One external render: a command template plus what to substitute into it.
struct RenderJob<'a> {
    spec: &'a CommandSpec,
    input: &'a Path,
    output: &'a Path,
    dpi: u32,
    /// `{display}` placeholder: display math (true) or inline (false).
    display: bool,
    /// Run with a scrubbed environment (hardened diagram mode).
    scrub_env: bool,
}

fn run_spec(ws: &Workspace, cfg: &Config, job: &RenderJob<'_>) -> Result<()> {
    let args: Vec<String> = job
        .spec
        .args
        .iter()
        .map(|a| substitute(a, job.input, job.output, job.dpi, job.display))
        .collect();
    let mut g = guard(ws, cfg, &[]);
    if job.scrub_env {
        g.scrub_env_to = Some(ws.path());
    }
    run_tool(&job.spec.program, &args, ws.path(), cfg.tool_timeout, &g)?;
    check_png(&job.spec.program, job.output, cfg.limits.max_image_bytes)
}

/// Render a Mermaid/Dolphin source to `<workspace>/graph_<hash>.png`.
pub fn render_graph(ws: &Workspace, kind: GraphKind, source: &str, cfg: &Config) -> Result<PathBuf> {
    if !cfg.graphs.enabled {
        return Err(CrateError::UnsafeContent("diagram rendering is disabled".into()));
    }
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
    let job = RenderJob {
        spec,
        input: &input,
        output: &output,
        dpi: cfg.graphs.dpi,
        display: false,
        scrub_env: cfg.graphs.hardened,
    };
    run_spec(ws, cfg, &job)?;
    Ok(output)
}

/// Render a fenced block with the embedder's [`FenceRenderer`](crate::FenceRenderer)
/// to `<workspace>/fence_<hash>.png`.
pub fn render_custom(ws: &Workspace, lang: &str, source: &str, cfg: &Config) -> Result<PathBuf> {
    let who = format!("fence renderer `{lang}`");
    let renderer = cfg
        .fence_renderers
        .get(lang)
        .ok_or_else(|| CrateError::InvalidConfig(format!("no fence renderer registered for `{lang}`")))?;
    let stem = asset_stem("fence", &[lang.as_bytes(), source.as_bytes()]);
    let output = ws.path().join(format!("{stem}.png"));
    if output.exists() {
        return Ok(output);
    }
    ws.check_budget(&who)?;
    // A renderer is arbitrary code: a panic becomes an error instead of unwinding
    // through the converter.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(source)));
    let failed = |detail: String| CrateError::ExternalToolFailed {
        program: who.clone(),
        detail,
    };
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return Err(failed(e.to_string())),
        Err(_) => return Err(failed("the renderer panicked".into())),
    };
    ws.check_budget(&who)?;
    // Check the size before the bytes reach the disk.
    if bytes.len() as u64 > cfg.limits.max_image_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "{who} returned {} bytes (limit {})",
            bytes.len(),
            cfg.limits.max_image_bytes
        )));
    }
    fs::write(&output, &bytes).map_err(|e| CrateError::io(&output, e))?;
    if dir_size(ws.path()) > cfg.limits.max_workspace_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "rendered images exceed {} bytes in total",
            cfg.limits.max_workspace_bytes
        )));
    }
    check_png(&who, &output, cfg.limits.max_image_bytes)?;
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
            crate::safety::check_tex(latex).map_err(CrateError::UnsafeContent)?;
            let tex = ws.path().join(format!("{stem}.tex"));
            fs::write(&tex, latex_document(latex, display)).map_err(|e| CrateError::io(&tex, e))?;
            // Relative to the working directory: paranoid mode refuses absolute paths,
            // including the main file's.
            let tex_arg = format!("{stem}.tex");
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
                &guard(ws, cfg, TEX_ENV),
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
                &guard(ws, cfg, &[]),
            )?;
            check_png(dvipng, &output, cfg.limits.max_image_bytes)?;
        }
        MathImageEngine::Command(spec) => {
            let input = ws.path().join(format!("{stem}.tex"));
            fs::write(&input, latex).map_err(|e| CrateError::io(&input, e))?;
            let job = RenderJob {
                spec,
                input: &input,
                output: &output,
                dpi,
                display,
                scrub_env: false,
            };
            run_spec(ws, cfg, &job)?;
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
