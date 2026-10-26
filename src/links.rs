//! Links to files on disk.
//!
//! By default a link must be an absolute address with an allowed scheme; a relative path such as
//! `docs/cli.md` is shown as text, because a `.docx` has no idea where it will be opened. For
//! documents you trust, [`LinkOptions`] turns such paths into hyperlinks that work:
//!
//! * [`LocalLinks::Local`] resolves each relative target against the Markdown file's directory,
//!   requires it to exist inside a *link root*, refuses executables, and rewrites it so it is
//!   correct from where the `.docx` will be saved ([`LinkStyle::Rebased`]) or as an absolute
//!   `file:` address ([`LinkStyle::Absolute`]). [`LocalLinks::All`] drops those checks.
//! * [`LinkOptions::base_url`] and [`LinkOptions::rewrites`] turn the same paths into web
//!   addresses (a repository's `blob/main/` URL, for instance).
//! * [`DocumentLayout`] makes a link to another Markdown file point at the `.docx` that file
//!   becomes, with `#heading` links landing on that heading in the other document.
//! * A [`LinkResolver`] decides any link yourself.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::diagnostics::Code;

/// Which local (non-URL) link targets become hyperlinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LocalLinks {
    /// None (the default): a relative or absolute path is shown as text.
    #[default]
    Off,
    /// Paths that name an existing file or directory inside [`LinkOptions::root`], other than
    /// executables and the like. A leading `/` means the root, as on GitHub. Anything else stays
    /// text and is reported.
    Local,
    /// Every path: also ones that do not exist, lie outside the root, are absolute, or are
    /// `file:` addresses. Network paths (`\\host\share`) are still refused: opening one sends
    /// the reader's Windows credentials to that host. Only for documents you wrote.
    All,
}

/// How a local target is written into the `.docx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LinkStyle {
    /// A path relative to the directory the `.docx` is saved in ([`LinkOptions::output_dir`]).
    /// Survives moving the whole tree, breaks if the `.docx` is moved alone.
    #[default]
    Rebased,
    /// A `file:///` address of the target. Survives moving the `.docx` anywhere on this machine,
    /// breaks on any other.
    Absolute,
}

/// Where the `.docx` files of a tree of Markdown files are written, so links between them (and
/// to files copied next to them) can be written correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentLayout {
    /// Directory the Markdown tree starts at; targets must lie inside it.
    pub source_root: PathBuf,
    /// Directory the `.docx` files go to.
    pub output_root: PathBuf,
    /// Keep the sub-directory structure (`docs/cli.md` becomes `output_root/docs/cli.docx`).
    /// `false` puts every document straight into `output_root`.
    pub mirror: bool,
    /// Other linked files and directories are copied to `output_root` (same relative path), so
    /// links to them point at the copy. Requires `mirror`. The copying itself is the caller's
    /// job (see [`crate::Report::local_links`]).
    pub copy_files: bool,
}

impl DocumentLayout {
    /// A layout that mirrors `source_root` under `output_root`.
    pub fn mirrored(source_root: impl Into<PathBuf>, output_root: impl Into<PathBuf>) -> Self {
        DocumentLayout {
            source_root: source_root.into(),
            output_root: output_root.into(),
            mirror: true,
            copy_files: false,
        }
    }

    /// The same layout with both directories written canonically (symbolic links resolved; the
    /// output directory need not exist yet). The library compares paths in this form.
    pub fn canonical(&self) -> DocumentLayout {
        DocumentLayout {
            source_root: lenient_canonical(&self.source_root),
            output_root: lenient_canonical(&self.output_root),
            ..self.clone()
        }
    }

    /// Where the `.docx` for the Markdown file `source` goes. `None` when `source` is outside
    /// the source root.
    pub fn document_path(&self, source: &Path) -> Option<PathBuf> {
        if self.mirror {
            let rel = source.strip_prefix(&self.source_root).ok()?;
            Some(self.output_root.join(rel).with_extension("docx"))
        } else {
            let stem = source.file_stem()?;
            Some(self.output_root.join(stem).with_extension("docx"))
        }
    }

    /// Where the copy of another linked file or directory goes.
    pub fn copy_path(&self, source: &Path) -> Option<PathBuf> {
        let rel = source.strip_prefix(&self.source_root).ok()?;
        Some(self.output_root.join(rel))
    }
}

/// How local links are handled. The defaults change nothing: see [`LocalLinks::Off`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LinkOptions {
    /// Which local targets become hyperlinks.
    pub local: LocalLinks,
    /// The tree local targets must stay inside (with [`LocalLinks::Local`]) and that URL rewrites
    /// are relative to. Default: the source root of [`LinkOptions::documents`] if set, else
    /// [`LinkOptions::source_dir`].
    pub root: Option<PathBuf>,
    /// Directory of the Markdown file; relative targets resolve against it. Default:
    /// [`Config::base_dir`](crate::Config) or the current directory. [`crate::convert_file`]
    /// sets it to the file's directory.
    pub source_dir: Option<PathBuf>,
    /// Directory the `.docx` will be saved in, which [`LinkStyle::Rebased`] needs. Default:
    /// `source_dir` (the document is saved next to its source).
    pub output_dir: Option<PathBuf>,
    /// Relative path or `file:` address; see [`LinkStyle`].
    pub style: LinkStyle,
    /// Web address that stands for the link root: a local file `docs/cli.md` becomes
    /// `{base_url}docs/cli.md` (and keeps its `#fragment`). Needs [`LinkOptions::local`] to be on.
    pub base_url: Option<String>,
    /// Like `base_url`, for directories (a repository's `tree/main/` rather than `blob/main/`).
    /// Falls back to `base_url`.
    pub dir_base_url: Option<String>,
    /// `(path prefix, web address)` pairs, first match wins, tried before `base_url`. The prefix
    /// is relative to the link root, with `/` separators and a trailing `/` for directories.
    pub rewrites: Vec<(String, String)>,
    /// Links between Markdown files in a tree that is converted together; see [`DocumentLayout`].
    pub documents: Option<DocumentLayout>,
    /// Also write, around every heading and HTML anchor, a bookmark that other documents can link
    /// to by heading (`other.docx#heading`). Implied by `documents`.
    pub export_anchors: bool,
    /// Check that `file.md#heading` names a heading that exists in that file (default on).
    pub check_anchors: bool,
}

impl Default for LinkOptions {
    fn default() -> Self {
        LinkOptions {
            local: LocalLinks::Off,
            root: None,
            source_dir: None,
            output_dir: None,
            style: LinkStyle::Rebased,
            base_url: None,
            dir_base_url: None,
            rewrites: Vec::new(),
            documents: None,
            export_anchors: false,
            check_anchors: true,
        }
    }
}

impl LinkOptions {
    /// Check the combination.
    pub fn validate(&self) -> Result<(), String> {
        let urls = self
            .base_url
            .iter()
            .chain(self.dir_base_url.iter())
            .chain(self.rewrites.iter().map(|(_, u)| u));
        for u in urls {
            if u.len() > 2000 || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
                return Err(
                    "a link base URL must be at most 2000 characters with no spaces or control characters".into(),
                );
            }
        }
        let wants_rewrite = self.base_url.is_some() || self.dir_base_url.is_some() || !self.rewrites.is_empty();
        if wants_rewrite && self.local == LocalLinks::Off {
            return Err("link base URLs and rewrites need local links to be on (LocalLinks::Local or All)".into());
        }
        if self.documents.is_some() && self.local == LocalLinks::Off {
            return Err("document links need local links to be on (LocalLinks::Local or All)".into());
        }
        if let Some(d) = &self.documents {
            if d.copy_files && !d.mirror {
                return Err("copying linked files needs a mirrored layout".into());
            }
        }
        Ok(())
    }
}

/// A local file or directory a document links to, found inside the link root.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LocalLink {
    /// The canonical path.
    pub path: PathBuf,
    /// A directory rather than a file.
    pub directory: bool,
    /// A Markdown file (`.md`, `.markdown`, ...): the kind of target another conversion can follow.
    pub markdown: bool,
}

/// The link being decided; see [`LinkResolver`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct LinkRequest<'a> {
    /// The destination exactly as written (a Markdown link's `(...)` or an HTML `href`).
    pub target: &'a str,
    /// From a raw HTML `<a href>` rather than Markdown link syntax.
    pub html: bool,
}

/// What a [`LinkResolver`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkDecision {
    /// No opinion: the built-in rules apply (scheme allow-list, [`LinkOptions`]).
    Default,
    /// Link to this address. You are trusted: no scheme list applies, but it must have no
    /// control characters and be at most 2048 bytes.
    Url(String),
    /// Link to another document, and to a heading in it: `target` is written as the link's
    /// address (typically a relative path to a `.docx`), `heading` is the heading's GitHub-style
    /// anchor in that document. The other document must have been converted with
    /// [`LinkOptions::export_anchors`] for the heading to be found.
    Document {
        /// The address of the other `.docx`.
        target: String,
        /// The heading's anchor, without `#`.
        heading: Option<String>,
    },
    /// Link to this document's own heading with this anchor (without `#`).
    Heading(String),
    /// Keep the link's text, drop the link.
    Text,
}

/// Decides what a link points at, before the built-in rules. Use it to map the paths in your
/// Markdown to addresses you control (wiki pages, document ids, a download service) without the
/// converter touching the file system. Called on the converting thread once per link; a panic is
/// caught and reported as an error.
pub trait LinkResolver: Send + Sync {
    /// Decide `request`.
    fn resolve(&self, request: &LinkRequest<'_>) -> LinkDecision;
}

impl<F> LinkResolver for F
where
    F: Fn(&LinkRequest<'_>) -> LinkDecision + Send + Sync,
{
    fn resolve(&self, request: &LinkRequest<'_>) -> LinkDecision {
        self(request)
    }
}

impl fmt::Debug for dyn LinkResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkResolver")
    }
}

// ------------------------------------------------------------------------------------ engine

/// The name of the bookmark other documents link to for a heading or anchor `name`.
pub(crate) fn exported_bookmark(name: &str) -> String {
    let digest = Sha256::digest(name.to_lowercase().as_bytes());
    let mut out = String::from("md_s");
    for b in &digest[..8] {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// What a local target turned into.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Local {
    /// Write the link: `target` as the address, `anchor` as the bookmark to land on.
    Link { target: String, anchor: Option<String> },
    /// Keep the text, drop the link.
    Text,
}

/// A resolved link and an optional finding to report with it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Resolution {
    pub local: Local,
    pub note: Option<(Code, String)>,
}

/// File extensions a link is never made to in [`LocalLinks::Local`]: things Windows or macOS run
/// or that carry macros when opened. (`All` links to anything.)
const BLOCKED_EXTENSIONS: &[&str] = &[
    "exe", "com", "bat", "cmd", "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "msi", "msp",
    "scr", "pif", "lnk", "url", "scf", "cpl", "dll", "sys", "reg", "jar", "app", "command", "docm", "dotm", "xlsm",
    "xlsb", "pptm", "ppsm", "xll", "wll", "iso", "dmg",
];

/// Most distinct linked Markdown files read to check `#heading` links in one conversion.
const MAX_CHECKED_FILES: usize = 256;

const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn"];

pub(crate) fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MARKDOWN_EXTENSIONS.iter().any(|m| m.eq_ignore_ascii_case(e)))
}

/// Resolves local targets for one conversion.
pub(crate) struct Engine {
    opts: LinkOptions,
    root: PathBuf,
    source_dir: PathBuf,
    output_dir: PathBuf,
    documents: Option<DocumentLayout>,
    sup_sub: bool,
    max_input: u64,
    anchors: HashMap<PathBuf, Option<HashSet<String>>>,
    found: Vec<LocalLink>,
    seen: HashSet<PathBuf>,
}

impl Engine {
    pub fn new(opts: &LinkOptions, base_dir: Option<&Path>, sup_sub: bool, max_input: usize) -> Engine {
        let cwd = || std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let source_dir = opts
            .source_dir
            .clone()
            .or_else(|| base_dir.map(Path::to_path_buf))
            .unwrap_or_else(cwd);
        let source_dir = lenient_canonical(&source_dir);
        // The tree being converted is the natural root when there is one.
        let root = opts
            .root
            .clone()
            .or_else(|| opts.documents.as_ref().map(|d| d.source_root.clone()))
            .unwrap_or_else(|| source_dir.clone());
        let root = lenient_canonical(&root);
        let output_dir = lenient_canonical(&opts.output_dir.clone().unwrap_or_else(|| source_dir.clone()));
        let documents = opts.documents.as_ref().map(DocumentLayout::canonical);
        Engine {
            opts: opts.clone(),
            root,
            source_dir,
            output_dir,
            documents,
            sup_sub,
            max_input: u64::try_from(max_input).unwrap_or(u64::MAX),
            anchors: HashMap::new(),
            found: Vec::new(),
            seen: HashSet::new(),
        }
    }

    pub fn exports_anchors(&self) -> bool {
        self.opts.export_anchors || self.documents.is_some()
    }

    pub fn take_found(&mut self) -> Vec<LocalLink> {
        std::mem::take(&mut self.found)
    }

    /// Whether `target` is something this engine handles: not an address with a scheme (other
    /// than `file:` in [`LocalLinks::All`]).
    pub fn is_local_target(&self, target: &str) -> bool {
        if self.opts.local == LocalLinks::Off {
            return false;
        }
        match scheme_of(target) {
            None => true,
            Some(s) => s.eq_ignore_ascii_case("file") && self.opts.local == LocalLinks::All,
        }
    }

    pub fn resolve(&mut self, raw: &str) -> Resolution {
        let all = self.opts.local == LocalLinks::All;
        let text = |code: Code, message: String| Resolution {
            local: Local::Text,
            note: Some((code, message)),
        };
        let shown = short(raw);

        // Split off `#fragment` and `?query`; neither is part of the file's name.
        let (path_part, fragment) = match raw.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (raw, None),
        };
        let path_part = path_part.split('?').next().unwrap_or("");
        let fragment = fragment.map(percent_decode).filter(|f| !f.is_empty());
        let mut decoded = percent_decode(path_part);
        if decoded.is_empty() || decoded.chars().any(|c| c.is_control()) {
            return text(
                Code::LinkBlocked,
                format!("the link target `{shown}` is empty or has control characters"),
            );
        }
        if let Some(rest) = strip_scheme(&decoded, "file") {
            // `file:///C:/x`, `file:///home/x`, `file://host/share` (a network path).
            let rest = rest.strip_prefix("//").unwrap_or(rest);
            if !rest.starts_with('/') && !rest.is_empty() && !is_drive(rest) {
                return text(
                    Code::LinkBlocked,
                    format!("the link `{shown}` names a network location, which is never linked"),
                );
            }
            decoded = rest.trim_start_matches('/').to_string();
            if !is_drive(&decoded) {
                decoded.insert(0, '/');
            }
        }
        let decoded = decoded.replace('\\', "/");
        if decoded.starts_with("//") {
            return text(
                Code::LinkBlocked,
                format!("the link `{shown}` names a network location, which is never linked"),
            );
        }

        // Which path on disk?
        let absolute_drive = is_drive(&decoded);
        let candidate: PathBuf = if absolute_drive {
            if !all {
                return text(
                    Code::LinkOutsideRoot,
                    format!("the link `{shown}` is an absolute path; only links inside the link root are made"),
                );
            }
            PathBuf::from(&decoded)
        } else if let Some(rooted) = decoded.strip_prefix('/') {
            // On GitHub a leading `/` is the repository root.
            let inside = self.root.join(rooted);
            if all && !exists(&lexical(&inside).unwrap_or_else(|| inside.clone())) {
                PathBuf::from(&decoded)
            } else {
                inside
            }
        } else {
            self.source_dir.join(&decoded)
        };
        let Some(normal) = lexical(&candidate) else {
            return text(
                Code::LinkOutsideRoot,
                format!("the link `{shown}` climbs above the top of the file system"),
            );
        };
        if has_network_prefix(&normal) {
            return text(
                Code::LinkBlocked,
                format!("the link `{shown}` names a network location, which is never linked"),
            );
        }
        let present = exists(&normal);
        let canon = if present {
            std::fs::canonicalize(&normal).unwrap_or(normal.clone())
        } else {
            lenient_canonical(&normal)
        };
        if has_network_prefix(&canon) {
            return text(
                Code::LinkBlocked,
                format!("the link `{shown}` leads to a network location, which is never linked"),
            );
        }
        let inside = canon.starts_with(&self.root);
        if !inside && !all {
            return text(
                Code::LinkOutsideRoot,
                format!(
                    "the link `{shown}` leaves the link root {}; only links inside it are made",
                    self.root.display()
                ),
            );
        }
        if !present && !all {
            return text(
                Code::LinkTargetMissing,
                format!("the link `{shown}` names a file or directory that does not exist"),
            );
        }
        let directory = present && canon.is_dir();
        if !all && !directory {
            let blocked = canon
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| BLOCKED_EXTENSIONS.iter().any(|b| b.eq_ignore_ascii_case(e)));
            if blocked {
                return text(
                    Code::LinkBlocked,
                    format!("the link `{shown}` names a program or macro document; it is not linked"),
                );
            }
        }
        let markdown = present && !directory && is_markdown_path(&canon);
        if present && inside && self.seen.insert(canon.clone()) {
            self.found.push(LocalLink {
                path: canon.clone(),
                directory,
                markdown,
            });
        }

        let mut notes: Option<(Code, String)> = None;
        if !present {
            notes = Some((
                Code::LinkTargetMissing,
                format!("the link `{shown}` names a file or directory that does not exist; it is linked anyway"),
            ));
        } else if markdown && self.opts.check_anchors {
            if let Some(frag) = &fragment {
                if !self.anchor_exists(&canon, frag) {
                    notes = Some((
                        Code::LinkAnchorMissing,
                        format!("{} has no heading with the anchor `{}`", short(&decoded), short(frag)),
                    ));
                }
            }
        }

        // The relative path inside the root, for rewrites.
        let rel_in_root = inside.then(|| {
            let rel = canon.strip_prefix(&self.root).unwrap_or(&canon);
            let mut s = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            if directory && !s.is_empty() {
                s.push('/');
            }
            s
        });

        // A web address, if the options say so.
        if let Some(rel) = &rel_in_root {
            if let Some(mut url) = self.rewrite(rel, directory) {
                if let Some(f) = &fragment {
                    url.push('#');
                    url.push_str(&encode_fragment(f));
                }
                return Local::Link {
                    target: url,
                    anchor: None,
                }
                .with_note(notes);
            }
        }

        // Another document in the tree being converted.
        if let (Some(layout), true) = (self.documents.clone(), markdown) {
            if let Some(out) = layout.document_path(&canon) {
                let anchor = fragment.as_ref().map(|f| exported_bookmark(f));
                return Local::Link {
                    target: self.address(&out, false),
                    anchor,
                }
                .with_note(notes);
            }
        }
        // A linked file copied next to the documents.
        if let Some(layout) = &self.documents {
            if layout.copy_files && inside {
                if let Some(copy) = layout.copy_path(&canon) {
                    return Local::Link {
                        target: self.address(&copy, directory),
                        anchor: None,
                    }
                    .with_note(notes);
                }
            }
        }

        // The file where it is. A `.docx` target keeps its heading; for anything else the
        // fragment would only confuse the program that opens it.
        let anchor = if canon
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            fragment.as_ref().map(|f| exported_bookmark(f))
        } else {
            None
        };
        Local::Link {
            target: self.address(&canon, directory),
            anchor,
        }
        .with_note(notes)
    }

    fn rewrite(&self, rel: &str, directory: bool) -> Option<String> {
        for (prefix, url) in &self.opts.rewrites {
            if let Some(rest) = rel.strip_prefix(prefix.as_str()) {
                return Some(format!("{url}{}", encode_path(rest)));
            }
        }
        let base = if directory {
            self.opts.dir_base_url.as_ref().or(self.opts.base_url.as_ref())
        } else {
            self.opts.base_url.as_ref()
        }?;
        Some(format!("{base}{}", encode_path(rel)))
    }

    /// `path` as the address to write, in the configured style.
    fn address(&self, path: &Path, directory: bool) -> String {
        if self.opts.style == LinkStyle::Rebased {
            if let Some(rel) = relative_path(&self.output_dir, path, directory) {
                return rel;
            }
        }
        file_uri(path, directory)
    }

    fn anchor_exists(&mut self, file: &Path, fragment: &str) -> bool {
        // Reading files for a check is bounded: past this many distinct files the rest go unchecked.
        if self.anchors.len() >= MAX_CHECKED_FILES && !self.anchors.contains_key(file) {
            return true;
        }
        let names = self.anchors.entry(file.to_path_buf()).or_insert_with(|| {
            let bytes = crate::safety::read_regular_file(file, &file.display().to_string(), self.max_input).ok()?;
            let text = String::from_utf8(bytes).ok()?;
            Some(crate::parser::anchor_names(&text, self.sup_sub))
        });
        match names {
            // Unreadable: do not report what we could not check.
            None => true,
            Some(set) => set.contains(&fragment.to_lowercase()),
        }
    }
}

impl Local {
    fn with_note(self, note: Option<(Code, String)>) -> Resolution {
        Resolution { local: self, note }
    }
}

// ---------------------------------------------------------------------------------- helpers

fn short(s: &str) -> String {
    let mut out: String = s.chars().filter(|c| !c.is_control()).take(80).collect();
    if s.chars().count() > 80 {
        out.push('\u{2026}');
    }
    out
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// The scheme of `target` (`http` in `http://x`), if it has one. A single letter followed by `:`
/// is a Windows drive, not a scheme.
fn scheme_of(target: &str) -> Option<&str> {
    let colon = target.find(':')?;
    let scheme = &target[..colon];
    let valid = scheme.len() > 1
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// What follows `scheme:` (case-insensitively) at the start of `s`.
fn strip_scheme<'a>(s: &'a str, scheme: &str) -> Option<&'a str> {
    let head = s.get(..scheme.len() + 1)?;
    (head[..scheme.len()].eq_ignore_ascii_case(scheme) && head.ends_with(':')).then(|| &s[scheme.len() + 1..])
}

/// `C:` / `C:/...`
fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'/' || b[2] == b'\\')
}

/// Whether `path` names something (a broken symbolic link names nothing).
fn exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// `path` with `.` and `..` resolved by text alone; `None` when a `..` would climb above the root.
fn lexical(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// The canonical form of the longest part of `path` that exists, followed by the rest.
pub(crate) fn lenient_canonical(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|c| c.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let normal = lexical(&absolute).unwrap_or(absolute);
    let mut existing = normal.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(&existing) {
            let mut out = c;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.file_name().map(|n| n.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return normal,
        }
    }
}

fn has_network_prefix(path: &Path) -> bool {
    use std::path::Prefix;
    matches!(
        path.components().next(),
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..))
    )
}

const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

fn encode_segment(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, SEGMENT).to_string()
}

/// A `/`-separated relative path with each segment encoded.
fn encode_path(rel: &str) -> String {
    rel.split('/').map(encode_segment).collect::<Vec<_>>().join("/")
}

fn encode_fragment(f: &str) -> String {
    encode_segment(f)
}

/// The path from directory `from` to `to`, as a relative address; `None` when they have no
/// common start (different drives).
fn relative_path(from: &Path, to: &Path, directory: bool) -> Option<String> {
    let a: Vec<Component> = from.components().collect();
    let b: Vec<Component> = to.components().collect();
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..a.len() {
        parts.push("..".into());
    }
    for c in &b[common..] {
        if let Component::Normal(n) = c {
            parts.push(encode_segment(&n.to_string_lossy()));
        }
    }
    let mut s = if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    };
    if directory && !s.ends_with('/') {
        s.push('/');
    }
    Some(s)
}

/// `file:///...` for an absolute path.
fn file_uri(path: &Path, directory: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut drive = String::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => {
                let s = p.as_os_str().to_string_lossy().into_owned();
                // `\\?\C:` (verbatim) and `C:` both become `C:`.
                drive = s.trim_start_matches(r"\\?\").to_string();
            }
            Component::Normal(n) => parts.push(encode_segment(&n.to_string_lossy())),
            _ => {}
        }
    }
    let mut uri = String::from("file:///");
    if !drive.is_empty() {
        uri.push_str(&drive);
        uri.push('/');
    }
    uri.push_str(&parts.join("/"));
    if directory && !uri.ends_with('/') {
        uri.push('/');
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_climb_and_descend() {
        let from = Path::new("/out/build");
        assert_eq!(
            relative_path(from, Path::new("/src/docs/a b.md"), false).unwrap(),
            "../../src/docs/a%20b.md"
        );
        assert_eq!(
            relative_path(from, Path::new("/out/build/x.docx"), false).unwrap(),
            "x.docx"
        );
        assert_eq!(relative_path(from, Path::new("/out/build"), true).unwrap(), "./");
        assert_eq!(
            relative_path(from, Path::new("/out/other/"), true).unwrap(),
            "../other/"
        );
    }

    #[test]
    fn file_uris_are_encoded() {
        assert_eq!(
            file_uri(Path::new("/home/me/my docs/a.md"), false),
            "file:///home/me/my%20docs/a.md"
        );
        assert_eq!(file_uri(Path::new("/home/me/dir"), true), "file:///home/me/dir/");
    }

    #[test]
    fn schemes_and_drives() {
        assert_eq!(scheme_of("https://x"), Some("https"));
        assert_eq!(scheme_of("docs/a.md"), None);
        assert_eq!(scheme_of("C:/x"), None);
        assert!(is_drive("C:/x") && is_drive("c:\\x") && !is_drive("cc:/x"));
        assert_eq!(strip_scheme("FILE:///x", "file"), Some("///x"));
    }

    #[test]
    fn exported_bookmarks_are_stable_valid_names_and_ignore_case() {
        let a = exported_bookmark("Exit-Status");
        assert_eq!(a, exported_bookmark("exit-status"));
        assert!(a.starts_with("md_s") && a.len() == 20 && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert_ne!(a, exported_bookmark("other"));
    }

    #[test]
    fn layouts_name_the_output() {
        let l = DocumentLayout::mirrored("/src", "/out");
        assert_eq!(
            l.document_path(Path::new("/src/docs/cli.md")).unwrap(),
            Path::new("/out/docs/cli.docx")
        );
        assert!(l.document_path(Path::new("/elsewhere/x.md")).is_none());
        let mut flat = l.clone();
        flat.mirror = false;
        assert_eq!(
            flat.document_path(Path::new("/src/docs/cli.md")).unwrap(),
            Path::new("/out/cli.docx")
        );
    }
}
