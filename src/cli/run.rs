//! Plan the jobs, run them, report each one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use silvercity_oss::{
    analyze_file, analyze_reader, convert_file, convert_reader, Config, CrateError, Diagnostic, DocumentLayout,
    LocalLink, Severity,
};

use super::args::{Args, Format, LinkLayout, MarkdownLinks};
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
    /// Where the `.docx` goes, or would go with `--check`: links are written relative to it.
    pub planned: Option<PathBuf>,
    /// Found by following a link rather than named on the command line.
    pub via_link: bool,
}

/// What a run needs beyond the jobs: the tree of documents, when links between them are rewritten.
pub struct Plan {
    pub jobs: VecDeque<Job>,
    pub layout: Option<DocumentLayout>,
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
pub fn plan(args: &Args) -> Result<Plan, CliError> {
    let stdin_count = args.inputs.iter().filter(|p| p.as_os_str() == "-").count();
    if stdin_count > 0 && args.inputs.len() > 1 {
        return Err(CliError::usage(
            "`-` (standard input) cannot be combined with other inputs",
        ));
    }
    if args.markdown_links != MarkdownLinks::Keep {
        return plan_documents(args, stdin_count > 0);
    }
    if args.check && args.output.is_some() {
        return Err(CliError::usage(
            "--check writes nothing, so -o has no effect (with --markdown-links it names where the links point)",
        ));
    }
    if args.copy_linked {
        return Err(CliError::usage("--copy-linked needs --markdown-links docx or follow"));
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

    let mut jobs = VecDeque::with_capacity(args.inputs.len());
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
        refuse_terminal(&dest)?;
        let planned = match &dest {
            Dest::File(p) => Some(p.clone()),
            _ => None,
        };
        jobs.push_back(Job {
            input: input.clone(),
            dest,
            planned,
            via_link: false,
        });
    }
    Ok(Plan { jobs, layout: None })
}

fn refuse_terminal(dest: &Dest) -> Result<(), CliError> {
    if *dest == Dest::Stdout && std::io::stdout().is_terminal() {
        return Err(CliError::usage(
            "refusing to write a .docx to a terminal; redirect standard output or use -o FILE",
        ));
    }
    Ok(())
}

/// `--markdown-links docx|follow`: the documents form a tree, rooted at the link root, whose
/// `.docx` files go under one output directory.
fn plan_documents(args: &Args, stdin: bool) -> Result<Plan, CliError> {
    if stdin {
        return Err(CliError::usage(
            "--markdown-links docx|follow needs files, not standard input: their place in the tree is what the links are written from",
        ));
    }
    let canonical: Vec<PathBuf> = args
        .inputs
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    let source_root = match &args.link_root {
        Some(r) => {
            std::fs::canonicalize(r).map_err(|e| CliError::usage(format!("--link-root {}: {e}", r.display())))?
        }
        None => common_directory(&canonical),
    };
    let output_root = match &args.output {
        Some(o) if o.as_os_str() == "-" => {
            return Err(CliError::usage(
                "--markdown-links docx|follow writes files; `-o -` is not possible",
            ));
        }
        Some(o) if !o.exists() || o.is_dir() => o.clone(),
        Some(o) => {
            return Err(CliError::usage(format!(
                "with --markdown-links docx|follow `-o` names a directory, and {} is a file",
                o.display()
            )))
        }
        None => source_root.clone(),
    };
    let mirror = args.link_layout == LinkLayout::Mirror;
    if args.copy_linked && (!mirror || output_root == source_root) {
        return Err(CliError::usage(
            "--copy-linked needs a mirrored layout and an output directory (-o DIR) other than the link root",
        ));
    }
    let mut layout = DocumentLayout::mirrored(&source_root, &output_root);
    layout.mirror = mirror;
    layout.copy_files = args.copy_linked;
    // The layout the library compares against is canonical; the output may not exist yet.
    let mut jobs = VecDeque::new();
    let mut claimed: HashMap<PathBuf, PathBuf> = HashMap::new();
    for (input, canon) in args.inputs.iter().zip(&canonical) {
        let job = document_job(input, canon, &layout, args.check, false)?;
        if let Some(out) = &job.planned {
            if let Some(other) = claimed.insert(out.clone(), canon.clone()) {
                if other != *canon {
                    return Err(CliError::usage(format!(
                        "{} and {} would both be written to {}",
                        other.display(),
                        canon.display(),
                        out.display()
                    )));
                }
            }
        }
        jobs.push_back(job);
    }
    Ok(Plan {
        jobs,
        layout: Some(layout),
    })
}

fn document_job(
    input: &Path,
    canon: &Path,
    layout: &DocumentLayout,
    check: bool,
    via_link: bool,
) -> Result<Job, CliError> {
    let canonical_layout = layout.canonical();
    let out = canonical_layout.document_path(canon).ok_or_else(|| {
        CliError::usage(format!(
            "{} is outside the link root {}",
            input.display(),
            layout.source_root.display()
        ))
    })?;
    // `document_path` joined onto the (possibly not yet existing) output root as given.
    let rel_out = out
        .strip_prefix(&canonical_layout.output_root)
        .map(|r| layout.output_root.join(r))
        .unwrap_or(out);
    Ok(Job {
        input: input.to_path_buf(),
        dest: if check {
            Dest::Nothing
        } else {
            Dest::File(rel_out.clone())
        },
        planned: Some(rel_out),
        via_link,
    })
}

/// The deepest directory that contains every path (their parents, for files).
fn common_directory(paths: &[PathBuf]) -> PathBuf {
    let mut it = paths.iter().map(|p| p.parent().unwrap_or(Path::new("/")).to_path_buf());
    let Some(mut common) = it.next() else {
        return PathBuf::from(".");
    };
    for dir in it {
        while !dir.starts_with(&common) {
            if !common.pop() {
                break;
            }
        }
    }
    common
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

/// Counters shared by every job of a run, for the limits on linked documents and copies.
#[derive(Default)]
pub struct Session {
    documents: usize,
    copied_files: usize,
    copied_bytes: u64,
    copied: HashSet<PathBuf>,
}

/// Do one job.
pub fn process(
    job: &Job,
    args: &Args,
    cfg: &Config,
    layout: Option<&DocumentLayout>,
    session: &mut Session,
) -> Result<Success, CliError> {
    let stdin = job.is_stdin();

    // A directory, a device or a missing file is a problem with the path, not with a document.
    if !stdin {
        match std::fs::metadata(&job.input) {
            Ok(m) if m.is_file() => {}
            Ok(_) => return Err(CliError::io(format!("{} is not a regular file", job.input.display()))),
            Err(e) => return Err(CliError::io(format!("cannot read {}: {e}", job.input.display()))),
        }
    }

    // Links are written from where this document will be saved.
    let mut cfg = cfg.clone();
    if let Some(out) = &job.planned {
        cfg.links.output_dir = out.parent().map(Path::to_path_buf);
    }
    cfg.links.documents = layout.cloned();

    if args.check {
        let report = if stdin {
            analyze_reader(std::io::stdin().lock(), &cfg)
        } else {
            analyze_file(&job.input, &cfg)
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
            local_links: report.local_links,
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
        convert_reader(std::io::stdin().lock(), &cfg)
    } else {
        convert_file(&job.input, &cfg)
    }?;
    match &job.dest {
        Dest::File(out) => {
            if layout.is_some() {
                if let Some(dir) = out.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| CliError::io(format!("cannot create {}: {e}", dir.display())))?;
                }
            }
            write_atomically(out, &conversion.docx)
                .map_err(|e| CliError::io(format!("cannot write {}: {e}", out.display())))?;
        }
        Dest::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(&conversion.docx)
                .and_then(|()| stdout.flush())
                .map_err(|e| CliError::io(format!("cannot write to standard output: {e}")))?;
        }
        Dest::Nothing => {}
    }
    if let (Some(layout), true) = (layout, args.copy_linked) {
        for link in conversion.local_links.iter().filter(|l| !l.markdown) {
            copy_linked(link, layout, args, session)?;
        }
    }
    Ok(Success {
        diagnostics: conversion.diagnostics,
        stats: conversion.stats,
        local_links: conversion.local_links,
    })
}

/// Copy a linked file, or a linked directory (without following symbolic links), to the same
/// relative path under the output directory.
fn copy_linked(link: &LocalLink, layout: &DocumentLayout, args: &Args, session: &mut Session) -> Result<(), CliError> {
    let canonical_layout = layout.canonical();
    let Some(rel_target) = canonical_layout.copy_path(&link.path) else {
        return Ok(());
    };
    let target = rel_target
        .strip_prefix(&canonical_layout.output_root)
        .map(|r| layout.output_root.join(r))
        .unwrap_or(rel_target);
    if !session.copied.insert(link.path.clone()) {
        return Ok(());
    }
    let too_many = || {
        CliError::new(
            super::exit::Exit::Rejected,
            "limit_exceeded",
            format!(
                "more than {} documents and copied files, or {} bytes copied (--max-linked-files, --max-copy-bytes)",
                args.max_linked_files, args.max_copy_bytes
            ),
        )
    };
    let mut stack = vec![(link.path.clone(), target)];
    while let Some((from, to)) = stack.pop() {
        let meta = std::fs::symlink_metadata(&from)
            .map_err(|e| CliError::io(format!("cannot read {}: {e}", from.display())))?;
        if meta.file_type().is_symlink() {
            continue; // a link inside a copied directory is not followed
        }
        if meta.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| CliError::io(format!("cannot create {}: {e}", to.display())))?;
            let entries =
                std::fs::read_dir(&from).map_err(|e| CliError::io(format!("cannot read {}: {e}", from.display())))?;
            for entry in entries {
                let entry = entry.map_err(|e| CliError::io(format!("cannot read {}: {e}", from.display())))?;
                stack.push((entry.path(), to.join(entry.file_name())));
            }
        } else if meta.is_file() {
            session.copied_files += 1;
            session.copied_bytes = session.copied_bytes.saturating_add(meta.len());
            if session.documents + session.copied_files > args.max_linked_files
                || session.copied_bytes > args.max_copy_bytes
            {
                return Err(too_many());
            }
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| CliError::io(format!("cannot create {}: {e}", dir.display())))?;
            }
            std::fs::copy(&from, &to)
                .map_err(|e| CliError::io(format!("cannot copy {} to {}: {e}", from.display(), to.display())))?;
        }
    }
    Ok(())
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
        say!(
            "{}",
            report::json(Some(&input), output.as_deref(), job.via_link, result)
        );
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
            report::json(
                None,
                None,
                false,
                &Err(CliError::new(e.exit, e.kind, e.message.clone()))
            )
        );
    } else {
        say!("error: {}", e.message);
    }
    e.exit.code()
}

/// Everything after argument parsing. The result is the process's exit status: that of the first
/// input that failed, after every input has been tried.
pub fn run(args: &Args, cfg: &Config) -> u8 {
    let Plan { mut jobs, layout } = match plan(args) {
        Ok(p) => p,
        Err(e) => return fatal(args, &e),
    };
    let follow = args.markdown_links == MarkdownLinks::Follow;
    let mut session = Session {
        documents: jobs.len(),
        ..Session::default()
    };
    let mut seen: HashSet<PathBuf> = jobs
        .iter()
        .map(|j| std::fs::canonicalize(&j.input).unwrap_or_else(|_| j.input.clone()))
        .collect();
    let mut claimed: HashMap<PathBuf, PathBuf> = jobs
        .iter()
        .filter_map(|j| {
            Some((
                j.planned.clone()?,
                std::fs::canonicalize(&j.input).unwrap_or_else(|_| j.input.clone()),
            ))
        })
        .collect();
    let many = jobs.len() > 1 || follow;
    let mut status = Exit::Ok.code();
    let note_failure = |code: u8, status: &mut u8| {
        if *status == 0 {
            *status = code;
        }
    };
    while let Some(job) = jobs.pop_front() {
        let result = process(&job, args, cfg, layout.as_ref(), &mut session);
        emit(&job, args, many, &result);
        match &result {
            Err(e) => note_failure(e.exit.code(), &mut status),
            Ok(ok) if follow => {
                let Some(layout) = layout.as_ref() else { continue };
                for link in ok.local_links.iter().filter(|l| l.markdown) {
                    if !seen.insert(link.path.clone()) {
                        continue;
                    }
                    if session.documents + session.copied_files >= args.max_linked_files {
                        let e = CliError::new(
                            Exit::Rejected,
                            "limit_exceeded",
                            format!(
                                "more than {} linked documents and copied files (--max-linked-files); {} and the rest were not converted",
                                args.max_linked_files,
                                link.path.display()
                            ),
                        );
                        let stop = Job {
                            input: link.path.clone(),
                            dest: Dest::Nothing,
                            planned: None,
                            via_link: true,
                        };
                        emit(&stop, args, many, &Err(e));
                        note_failure(Exit::Rejected.code(), &mut status);
                        jobs.clear();
                        break;
                    }
                    match document_job(&link.path, &link.path, layout, args.check, true) {
                        Ok(next) => {
                            if let Some(out) = &next.planned {
                                if let Some(other) = claimed.insert(out.clone(), link.path.clone()) {
                                    if other != link.path {
                                        let e = CliError::usage(format!(
                                            "{} and {} would both be written to {}",
                                            other.display(),
                                            link.path.display(),
                                            out.display()
                                        ));
                                        emit(&next, args, many, &Err(e));
                                        note_failure(Exit::Usage.code(), &mut status);
                                        continue;
                                    }
                                }
                            }
                            session.documents += 1;
                            jobs.push_back(next);
                        }
                        Err(e) => {
                            let skipped = Job {
                                input: link.path.clone(),
                                dest: Dest::Nothing,
                                planned: None,
                                via_link: true,
                            };
                            emit(&skipped, args, many, &Err(e.clone_error()));
                            note_failure(e.exit.code(), &mut status);
                        }
                    }
                }
            }
            Ok(_) => {}
        }
    }
    status
}
