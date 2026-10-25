//! Markdown to Word (`.docx`) with native handling of LaTeX math and
//! text-based diagrams.
//!
//! ```text
//! Markdown -> [1 preprocess: render math/graphs, rewrite as images]
//!          -> [2 pulldown-cmark events] -> [3 docx-rs writer] -> .docx bytes
//! ```
//!
//! * ```` ```mermaid ```` / ```` ```dolphin ```` fences are rendered to PNG by
//!   an external CLI (`mmdc` by default) and embedded as pictures.
//! * `$..$` and `$$..$$` become either transparent PNGs rendered by an
//!   external tool ([`MathMode::ImageFallback`], the default) or native,
//!   editable Word equations ([`MathMode::NativeOmml`], no external tool).
//!
//! All intermediate files live in a `tempfile::TempDir` that is removed when
//! the conversion returns, on success and on error.

mod config;
mod error;
mod html;
pub mod omml;
mod parser;
mod preprocess;
pub mod renderer;
mod safety;

use std::fs;
use std::path::Path;

pub use config::{CommandSpec, Config, GraphOptions, ImageAccess, Limits, MathImageEngine, MathMode, MathOptions};
pub use error::{CrateError, Result};
pub use preprocess::{preprocess, Asset, AssetKind, AssetSource, Preprocessed};

/// Convert Markdown text to the bytes of a `.docx` file.
pub fn markdown_to_docx(markdown: &str, config: &Config) -> Result<Vec<u8>> {
    let pre = preprocess(markdown, config)?;
    parser::build_docx(&pre, config)
}

/// Convert a Markdown file. Relative image paths resolve against the file's
/// directory unless `config.base_dir` is set.
pub fn markdown_file_to_docx(path: &Path, config: &Config) -> Result<Vec<u8>> {
    let bytes = fs::read(path).map_err(|e| CrateError::io(path, e))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CrateError::InvalidMarkdown(format!("{} is not valid UTF-8", path.display())))?;
    let mut cfg = config.clone();
    if cfg.base_dir.is_none() {
        cfg.base_dir = path.parent().map(Path::to_path_buf);
    }
    markdown_to_docx(&text, &cfg)
}
