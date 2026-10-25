//! Markdown to Word (`.docx`) with native handling of LaTeX math and
//! text-based diagrams, built to be safe on untrusted input.
//!
//! ```text
//! Markdown -> [1 preprocess: render math/graphs, rewrite as images]
//!          -> [2 pulldown-cmark events] -> [3 docx-rs writer] -> .docx bytes
//! ```
//!
//! # Quick start
//!
//! ```
//! use silvercity_oss::{markdown_to_docx, Config};
//!
//! # fn main() -> Result<(), silvercity_oss::CrateError> {
//! let docx: Vec<u8> = markdown_to_docx("# Hello\n\nEuler: $e^{i\\pi} + 1 = 0$", &Config::default())?;
//! assert_eq!(&docx[..2], b"PK"); // a zip archive, i.e. a .docx file
//! # Ok(()) }
//! ```
//!
//! # Untrusted input
//!
//! Use [`Config::untrusted`] for Markdown you do not control. It starts no
//! subprocess, reads no local file and enforces tight size limits; raw HTML is
//! reduced to a safe subset. Every failure is a [`CrateError`]; the crate does
//! not panic on bad input.
//!
//! ```
//! use silvercity_oss::{markdown_to_docx, Config, CrateError};
//!
//! let hostile = "[x](javascript:alert(1)) <script>bad()</script> ![secret](/etc/passwd)";
//! let docx = markdown_to_docx(hostile, &Config::untrusted()).expect("safe by construction");
//! # let _ = docx;
//!
//! // Limits turn abuse into errors, not hangs or crashes.
//! let huge = "x".repeat(3 * 1024 * 1024);
//! assert!(matches!(markdown_to_docx(&huge, &Config::untrusted()), Err(CrateError::LimitExceeded(_))));
//! ```
//!
//! # Page, fonts and writers
//!
//! ```
//! use silvercity_oss::{markdown_to_docx_writer, Config, DocumentOptions, PageSize};
//!
//! # fn main() -> Result<(), silvercity_oss::CrateError> {
//! let mut document = DocumentOptions::default();
//! document.page = PageSize::Letter;
//! document.body_font = "Georgia".into();
//! let config = Config::untrusted().with_document(document);
//!
//! let mut out = std::io::Cursor::new(Vec::new()); // or a std::fs::File
//! markdown_to_docx_writer("# Letter-size page", &config, &mut out)?;
//! assert!(out.into_inner().len() > 1000);
//! # Ok(()) }
//! ```
//!
//! # Things to know
//!
//! * **Blocking.** Conversion is synchronous and may wait on external tools
//!   (diagrams, image-mode math). From async code, call it inside
//!   `spawn_blocking`.
//! * **Thread-safe.** [`Config`] is `Send + Sync`; share one across threads.
//!   Each call uses its own scratch directory, removed when the call returns.
//! * **Not byte-reproducible.** The same input gives the same *content* every
//!   time, but `docx-rs` numbers relationship, footnote and picture ids from
//!   process-wide counters, so two calls can differ in those ids (and in the
//!   order of relationship entries). Compare or cache by content, not by
//!   hashing the bytes.
//! * **Features.** `cli` (default) builds the `md2docx` binary. Library users
//!   should depend on the crate with `default-features = false` to avoid
//!   compiling `clap` and `shlex`.
//! * **Syntax highlighting** for code blocks: [`Highlighting`], [`Theme`], [`Highlighter`].
//! * **Extensible by the embedder, not the document author.**
//!   [`Config::with_fence_renderer`] renders fenced blocks with your own code and
//!   [`Config::with_html_tag`] styles custom HTML tags; both are safe with
//!   [`Config::untrusted`].
//! * **Diagram and TeX tools are external** programs, never linked in. See
//!   [`MermaidEngine`] and [`MathImageEngine`].
//!
//! The whole crate is `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod control;
mod docx;
mod error;
mod extend;
mod highlight;
mod html;
mod images;
mod lexers;
pub mod omml;
mod pack;
mod parser;
mod preprocess;
mod renderer;
mod safety;

/// Not part of the public API: internals that the crate's own integration tests
/// reach into. No stability promise of any kind; use [`markdown_to_docx`] and the
/// other documented functions.
#[doc(hidden)]
pub mod __internals {
    pub use crate::preprocess::{preprocess, Asset, AssetKind, AssetSource, Preprocessed};
    pub use crate::renderer::{run_tool, ToolGuard, TEX_ENV};
}

use std::io::{Seek, Write};
use std::path::Path;

pub use config::{
    CodeBlockLayout, CommandSpec, Config, DiagramFailure, DocumentOptions, GraphOptions, ImageAccess, Limits,
    MathImageEngine, MathMode, MathOptions, MermaidEngine, PageSize,
};
pub use control::CancelToken;
pub use error::{CrateError, Result};
pub use extend::{BlockStyle, FenceError, FenceRenderer, FenceRenderers, RenderContext, TagStyle};
pub use highlight::{BuiltinHighlighter, Highlighter, Highlighting, Span, Theme, TokenClass, TokenStyle};
pub use renderer::sweep_scratch;

/// Convert Markdown text to the bytes of a `.docx` file.
///
/// Returns [`CrateError::InvalidConfig`] for out-of-range options before doing
/// any work.
pub fn markdown_to_docx(markdown: &str, config: &Config) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    markdown_to_docx_writer(markdown, config, &mut out)?;
    Ok(out.into_inner())
}

/// Convert Markdown text and write the `.docx` archive to `writer`
/// (a `File`, a `Cursor<Vec<u8>>`, ...). Nothing is written if conversion fails
/// before packaging. If packaging itself fails (including the result exceeding
/// `Limits::max_output_bytes`), the writer may hold a partial archive: discard it.
pub fn markdown_to_docx_writer<W: Write + Seek>(markdown: &str, config: &Config, writer: W) -> Result<()> {
    config.validate()?;
    let ctl = control::Ctl::new(config);
    let pre = preprocess::preprocess_with(markdown, config, &ctl)?;
    parser::build_docx(&pre, config, writer)
}

/// Convert a Markdown file. Relative image paths resolve against the file's
/// directory unless `config.base_dir` is set.
pub fn markdown_file_to_docx(path: &Path, config: &Config) -> Result<Vec<u8>> {
    // Bounded read: a huge file or a device like /dev/zero must not be slurped into memory.
    let max = u64::try_from(config.limits.max_input_bytes).unwrap_or(u64::MAX);
    let bytes = safety::read_regular_file(path, &path.display().to_string(), max)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CrateError::InvalidMarkdown(format!("{} is not valid UTF-8", path.display())))?;
    let mut cfg = config.clone();
    if cfg.base_dir.is_none() {
        // A bare file name has the empty path as its parent: that means the current directory.
        cfg.base_dir = path
            .parent()
            .map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p })
            .map(Path::to_path_buf);
    }
    markdown_to_docx(&text, &cfg)
}
