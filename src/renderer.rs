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
use crate::control::{CancelToken, Ctl};
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

    pub(crate) fn name(self) -> &'static str {
        match self {
            GraphKind::Mermaid => "mermaid",
            GraphKind::Dolphin => "dolphin",
        }
    }
}

/// Name prefix of every scratch directory this crate creates.
pub(crate) const SCRATCH_PREFIX: &str = "md2docx-";
/// File created inside each scratch directory so [`sweep_scratch`] only ever removes directories
/// this crate made.
pub(crate) const SCRATCH_MARKER: &str = ".md2docx-scratch";

/// RAII scratch directory. Dropping it deletes every rendered asset.
#[derive(Debug)]
pub struct Workspace {
    dir: TempDir,
    /// When the conversion's total external-tool time is used up.
    deadline: Option<Instant>,
    ctl: Ctl,
}

impl Workspace {
    #[cfg(test)]
    pub fn new() -> Result<Self> {
        Self::create(&Config::trusted(), &Ctl::none())
    }

    /// A private directory under [`Config::scratch_dir`] (or the system temporary directory)
    /// whose tools may run for `limits.max_total_tool_time` in total.
    pub(crate) fn create(cfg: &Config, ctl: &Ctl) -> Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(SCRATCH_PREFIX);
        let dir = match &cfg.scratch_dir {
            Some(root) => builder.tempdir_in(root).map_err(|e| CrateError::io(root, e))?,
            None => builder.tempdir().map_err(|e| CrateError::io(std::env::temp_dir(), e))?,
        };
        let marker = dir.path().join(SCRATCH_MARKER);
        fs::write(&marker, b"").map_err(|e| CrateError::io(&marker, e))?;
        Ok(Workspace {
            dir,
            deadline: Instant::now().checked_add(cfg.limits.max_total_tool_time),
            ctl: ctl.clone(),
        })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Fail when the host cancelled the conversion, its deadline passed, or the total time for
    /// external work is used up.
    pub fn check_budget(&self, what: &str) -> Result<()> {
        self.ctl.check()?;
        match self.deadline {
            Some(d) if Instant::now() >= d => Err(CrateError::LimitExceeded(format!(
                "the time budget for external work ran out before {what} finished"
            ))),
            _ => Ok(()),
        }
    }
}

/// Remove scratch directories that earlier conversions left under `root` (the directory you gave
/// [`Config::scratch_dir`]): the leftovers of a worker that was killed from outside, whose
/// destructors never ran. Returns how many were removed.
///
/// Only directories this crate created are touched: the name starts with `md2docx-`, it holds
/// the crate's marker file, it is a real directory (never a symbolic link), and it was last
/// modified more than `older_than` ago. Pick `older_than` comfortably longer than
/// [`Limits::max_conversion_time`](crate::Limits) so a conversion still running is not disturbed,
/// or call it only when no worker is using `root`.
pub fn sweep_scratch(root: &Path, older_than: Duration) -> Result<usize> {
    let entries = fs::read_dir(root).map_err(|e| CrateError::io(root, e))?;
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(SCRATCH_PREFIX) {
            continue;
        }
        let path = entry.path();
        // `symlink_metadata`: a link that points at a directory is not ours to follow.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() || fs::symlink_metadata(path.join(SCRATCH_MARKER)).is_err() {
            continue;
        }
        let age = meta.modified().ok().and_then(|m| m.elapsed().ok());
        if age.is_some_and(|a| a >= older_than) && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
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
    /// Kill the tool and fail with [`CrateError::Cancelled`] once this token is set.
    pub cancel: Option<&'a CancelToken>,
    /// Kill the tool and fail with [`CrateError::TimedOut`] after this instant: the conversion's
    /// own deadline.
    pub conversion_deadline: Option<Instant>,
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

/// Owns a started tool. Dropping it kills the tool's whole process group (Chrome behind `mmdc`,
/// for example) and reaps it, so **every** exit from `run_tool` after the spawn (an early
/// `return`, a `?`, a panic) ends the process tree. The explicit [`ProcessGuard::kill_tree`] calls
/// only make the order of events deterministic; they are not what the safety rests on.
struct ProcessGuard {
    child: Option<std::process::Child>,
}

impl ProcessGuard {
    fn new(child: std::process::Child) -> Self {
        ProcessGuard { child: Some(child) }
    }

    fn child(&mut self) -> &mut std::process::Child {
        self.child.as_mut().expect("the child is only taken on drop")
    }

    /// Kill the tool and everything in its process group, then reap it. Safe to call twice.
    fn kill_tree(&mut self) {
        let Some(child) = self.child.as_mut() else { return };
        #[cfg(unix)]
        if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
            // The child leads its own process group (see `spawn`), so this reaches its
            // descendants, including ones that outlive it. The group id stays reserved while
            // any member is alive; if none is, the signal finds no target.
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.kill_tree();
        self.child = None;
    }
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

    if guard.cancel.is_some_and(CancelToken::is_cancelled) {
        return Err(CrateError::Cancelled("the conversion was cancelled".into()));
    }
    if guard.overall_deadline.is_some_and(|d| Instant::now() >= d) {
        return Err(budget_exhausted());
    }
    let mut log = tempfile::tempfile_in(cwd).map_err(|e| CrateError::io(cwd, e))?;

    let mut attempt = 0u32;
    let child = loop {
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
                return Err(CrateError::MissingExternalCli {
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

    // From here on the guard ends the whole process tree on every path out of this function.
    let mut proc = ProcessGuard::new(child);
    let own_deadline = Instant::now() + timeout;
    let log_too_big = |log: &fs::File| log.metadata().map(|m| m.len()).unwrap_or(0) > MAX_TOOL_LOG_BYTES;
    let status = loop {
        match proc.child().try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if guard.cancel.is_some_and(|c| c.is_cancelled()) {
                    return Err(CrateError::Cancelled("the conversion was cancelled".into()));
                }
                if guard.conversion_deadline.is_some_and(|d| now >= d) {
                    return Err(crate::control::timed_out());
                }
                if guard.overall_deadline.is_some_and(|d| now >= d) {
                    return Err(budget_exhausted());
                }
                if now >= own_deadline {
                    return Err(tool_err(format!("timed out after {}s", timeout.as_secs())));
                }
                if log_too_big(&log) {
                    return Err(tool_err("killed: more than 1 MiB of console output".into()));
                }
                if dir_size(cwd) > guard.max_dir_bytes {
                    return Err(CrateError::LimitExceeded(format!(
                        "`{program}` wrote more than {} bytes of files",
                        guard.max_dir_bytes
                    )));
                }
                sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(tool_err(format!("wait failed: {e}"))),
        }
    };

    // The tool is gone, but helpers it started may not be: end them first, whatever the tool's own
    // outcome and before any check below can return.
    proc.kill_tree();

    // Polling can miss a tool that writes faster than the poll interval (a fast disk writes
    // hundreds of MB in a few milliseconds), so enforce both caps once more now that it has
    // exited. The kill above only limits how far it can overshoot.
    if log_too_big(&log) {
        return Err(tool_err("more than 1 MiB of console output".into()));
    }
    if dir_size(cwd) > guard.max_dir_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "`{program}` wrote more than {} bytes of files",
            guard.max_dir_bytes
        )));
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

fn guard<'a>(ws: &'a Workspace, cfg: &Config, env: &'a [(&'a str, &'a str)]) -> ToolGuard<'a> {
    ToolGuard {
        env,
        max_dir_bytes: cfg.limits.max_workspace_bytes,
        overall_deadline: ws.deadline,
        cancel: ws.ctl.cancel_token(),
        conversion_deadline: ws.ctl.deadline(),
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
    ws.check_budget("the diagram")?;
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

/// Render a fenced block with the embedder's [`FenceRenderer`](crate::FenceRenderer).
///
/// The PNG stays in memory: nothing is written to the scratch directory, so there is no file to
/// leave behind, no disk quota to spend and no path for a later stage to be tricked into reading.
pub fn render_custom(ctl: &Ctl, lang: &str, source: &str, cfg: &Config) -> Result<Vec<u8>> {
    let who = format!("fence renderer `{lang}`");
    let renderer = cfg
        .fence_renderers
        .get(lang)
        .ok_or_else(|| CrateError::InvalidConfig(format!("no fence renderer registered for `{lang}`")))?;
    ctl.check()?;
    let ctx = crate::extend::RenderContext::new(ctl, cfg);
    // A renderer is arbitrary code: a panic becomes an error instead of unwinding
    // through the converter.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render_with(source, &ctx)));
    // A renderer that ignored the deadline or the cancellation still does not get its result used.
    ctl.check()?;
    let failed = |detail: String| CrateError::ExternalToolFailed {
        program: who.clone(),
        detail,
    };
    let bytes = match result {
        Ok(Ok(bytes)) => bytes,
        // A renderer reports "the document is over a limit" by returning that error itself.
        Ok(Err(e)) => match e.downcast::<CrateError>() {
            Ok(inner) if matches!(*inner, CrateError::LimitExceeded(_)) => return Err(*inner),
            Ok(inner) => return Err(failed(inner.to_string())),
            Err(other) => return Err(failed(other.to_string())),
        },
        Err(_) => return Err(failed("the renderer panicked".into())),
    };
    if bytes.len() as u64 > cfg.limits.max_image_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "{who} returned {} bytes (limit {})",
            bytes.len(),
            cfg.limits.max_image_bytes
        )));
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(failed("did not return a PNG".into()));
    }
    Ok(bytes)
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
    ws.check_budget("the formula")?;
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
