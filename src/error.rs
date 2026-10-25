//! Error type shared by every stage of the pipeline.

use std::path::PathBuf;

/// Every way a conversion can fail. The crate never panics on bad input; it
/// returns one of these instead.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CrateError {
    /// A required external program (e.g. `mmdc`, `latex`, `dvipng`) is not on
    /// `PATH` (or the configured path does not exist).
    #[error("external CLI `{program}` was not found; install it or point the configuration at it")]
    MissingExternalCLI { program: String },

    /// The Markdown input could not be processed (not UTF-8, empty graph
    /// block, malformed math, ...).
    #[error("invalid markdown: {0}")]
    InvalidMarkdown(String),

    /// The input exceeds a configured [`Limits`](crate::Limits) value.
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),

    /// The input asks for something the active security policy forbids
    /// (a TeX file-access primitive, an image outside the base directory, ...).
    #[error("blocked by security policy: {0}")]
    UnsafeContent(String),

    /// The Word document could not be assembled or packed.
    #[error("docx generation failed: {0}")]
    DocxGenerationError(String),

    /// An external renderer started but failed, timed out, or produced no
    /// usable image.
    #[error("external tool `{program}` failed: {detail}")]
    ExternalToolFailed { program: String, detail: String },

    /// Reading an input file or an image referenced by the document failed.
    #[error("i/o error on {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl CrateError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CrateError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CrateError>;
