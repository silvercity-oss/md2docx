//! Plan the jobs, run them, report each one.

use std::collections::HashSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use silvercity_oss::{
    analyze_file, analyze_reader, convert_file, convert_reader, Config, CrateError, Diagnostic, Severity,
};

use super::args::{Args, Format};
use super::exit::{CliError, Exit};
use super::report::{self, Success};

/// Where one input's document goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    File(PathBuf),
    Stdout,
    /// `--check`: nowhere.
    Nothing,
}

#[derive(Debug)]
pub struct Job {
    /// `-` is standard input.
    pub input: PathBuf,
    pub dest: Dest,
}

impl Job {
    fn is_stdin(&self) -> bool {
        self.input.as_os_str() == "-"
    }

    fn label(&self) -> String {
        if self.is_stdin() {
            "<stdin>".into()
        } else {
            self.input.display().to_string()
        }
    }
}

/// Decide, before any work starts, which input becomes which output. Every rule that can be
/// checked from the command line alone is checked here, so a mistake costs nothing.
pub fn plan(args: &Args) -> Result<Vec<Job>, CliError> {
    let stdin_count = args.inputs.iter().filter(|p| p.as_os_str() == "-").count();
    if stdin_count > 0 && args.inputs.len() > 1 {
        return Err(CliError::usage(
            "`-` (standard input) cannot be combined with other inputs",
        ));
    }
    let many = args.inputs.len() > 1;
    let out_is_stdout = args.output.as_deref().is_some_and(|o| o.as_os_str() == "-");
    if out_is_stdout && many {
        return Err(CliError::usage(
            "`-o -` writes one document; give a directory for several inputs",
        ));
    }
    let out_dir = match &args.output {
        Some(o) if !out_is_stdout && (many || o.is_dir()) => {
            if !o.is_dir() {
                return Err(CliError::usage(format!(
                    "with several inputs `-o` must be an existing directory, and {} is not",
                    o.display()
                )));
            }
            Some(o.clone())
        }
        _ => None,
    };

    let mut jobs = Vec::with_capacity(args.inputs.len());
    let mut taken: HashSet<PathBuf> = HashSet::new();
    for input in &args.inputs {
        let stdin = input.as_os_str() == "-";
        let dest = if args.check {
            Dest::Nothing
        } else if stdin {
            match &args.output {
                Some(o) if !out_is_stdout => Dest::File(o.clone()),
                _ => Dest::Stdout,
            }
        } else if out_is_stdout {
            Dest::Stdout
        } else {
            let name = input
                .file_stem()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CliError::usage(format!("cannot name an output for {}", input.display())))?;
            match (&out_dir, &args.output) {
                (Some(dir), _) => Dest::File(dir.join(name).with_extension("docx")),
                (None, Some(file)) => Dest::File(file.clone()),
                (None, None) => Dest::File(input.with_extension("docx")),
            }
        };
        if let Dest::File(p) = &dest {
            if !taken.insert(p.clone()) {
                return Err(CliError::usage(format!(
                    "two inputs would both be written to {}",
                    p.display()
                )));
            }
        }
        if dest == Dest::Stdout && std::io::stdout().is_terminal() {
            return Err(CliError::usage(
                "refusing to write a .docx to a terminal; redirect standard output or use -o FILE",
            ));
        }
        jobs.push(Job {
            input: input.clone(),
            dest,
        });
    }
    Ok(jobs)
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
pub fn write_atomically(out: &Path, bytes: &[u8]) -> std::io::Result<()> {
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

fn has_warnings(list: &[Diagnostic]) -> bool {
    list.iter().any(|d| d.severity >= Severity::Warning)
}

/// Do one job.
pub fn process(job: &Job, args: &Args, cfg: &Config) -> Result<Success, CliError> {
    let stdin = job.is_stdin();

    // A directory, a device or a missing file is a problem with the path, not with a document.
    if !stdin {
        match std::fs::metadata(&job.input) {
            Ok(m) if m.is_file() => {}
            Ok(_) => return Err(CliError::io(format!("{} is not a regular file", job.input.display()))),
            Err(e) => return Err(CliError::io(format!("cannot read {}: {e}", job.input.display()))),
        }
    }

    if args.check {
        let report = if stdin {
            analyze_reader(std::io::stdin().lock(), cfg)
        } else {
            analyze_file(&job.input, cfg)
        }?;
        if args.strict && has_warnings(&report.diagnostics) {
            return Err(CrateError::LossyContent {
                diagnostics: report.diagnostics,
            }
            .into());
        }
        return Ok(Success {
            diagnostics: report.diagnostics,
            stats: report.stats,
        });
    }

    if let (Dest::File(out), false) = (&job.dest, stdin) {
        if is_same_file(&job.input, out) {
            return Err(CliError::usage(format!(
                "refusing to overwrite the input file {}",
                job.input.display()
            )));
        }
    }
    let conversion = if stdin {
        convert_reader(std::io::stdin().lock(), cfg)
    } else {
        convert_file(&job.input, cfg)
    }?;
    match &job.dest {
        Dest::File(out) => write_atomically(out, &conversion.docx)
            .map_err(|e| CliError::io(format!("cannot write {}: {e}", out.display())))?,
        Dest::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(&conversion.docx)
                .and_then(|()| stdout.flush())
                .map_err(|e| CliError::io(format!("cannot write to standard output: {e}")))?;
        }
        Dest::Nothing => {}
    }
    Ok(Success {
        diagnostics: conversion.diagnostics,
        stats: conversion.stats,
    })
}

/// Print one job's outcome in the chosen format.
fn emit(job: &Job, args: &Args, many: bool, result: &Result<Success, CliError>) {
    let label = job.label();
    if args.format == Format::Json {
        let output = match &job.dest {
            Dest::File(p) => Some(p.display().to_string()),
            Dest::Stdout => Some("-".to_string()),
            Dest::Nothing => None,
        };
        let input = if job.is_stdin() {
            "-".to_string()
        } else {
            job.input.display().to_string()
        };
        say!("{}", report::json(Some(&input), output.as_deref(), result));
        return;
    }
    match result {
        Ok(ok) => {
            if args.quiet {
                return;
            }
            for d in ok.diagnostics.iter().filter(|d| d.severity >= Severity::Warning) {
                say!("{}", report::describe(&label, d));
            }
            match &job.dest {
                Dest::File(p) => say!("wrote {}", p.display()),
                Dest::Nothing => {
                    let n = ok
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity >= Severity::Warning)
                        .count();
                    match n {
                        0 => say!("checked {label}: no problems"),
                        n => say!("checked {label}: {n} warning(s)"),
                    }
                }
                Dest::Stdout => {}
            }
        }
        Err(e) => {
            for d in &e.diagnostics {
                say!("{}", report::describe(&label, d));
            }
            if many {
                say!("error: {label}: {}", e.message);
            } else {
                say!("error: {}", e.message);
            }
        }
    }
}

/// Report a failure that happened before any input was looked at.
pub fn fatal(args: &Args, e: &CliError) -> u8 {
    if args.format == Format::Json {
        say!(
            "{}",
            report::json(None, None, &Err(CliError::new(e.exit, e.kind, e.message.clone())))
        );
    } else {
        say!("error: {}", e.message);
    }
    e.exit.code()
}

/// Everything after argument parsing. The result is the process's exit status: that of the first
/// input that failed, after every input has been tried.
pub fn run(args: &Args, cfg: &Config) -> u8 {
    let jobs = match plan(args) {
        Ok(j) => j,
        Err(e) => return fatal(args, &e),
    };
    let many = jobs.len() > 1;
    let mut status = Exit::Ok.code();
    for job in &jobs {
        let result = process(job, args, cfg);
        emit(job, args, many, &result);
        if let (Err(e), 0) = (&result, status) {
            status = e.exit.code();
        }
    }
    status
}
