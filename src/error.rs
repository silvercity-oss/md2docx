//! Error type shared by every stage of the pipeline.

use std::path::PathBuf;

/// Every way a conversion can fail. The crate never panics on bad input; it
/// returns one of these instead.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CrateError {
    /// A required external program (e.g. `mmdc`, `latex`, `dvipng`) is not on
    /// `PATH` (or the configured path does not exist).
    #[error("external CLI `{program}` was not found{}", install_hint(program))]
    MissingExternalCLI {
        /// The program that could not be started, as configured.
        program: String,
    },

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

    /// A [`Config`](crate::Config) value is out of range (page size, dpi, ...).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// The Word document could not be assembled or packed.
    #[error("docx generation failed: {0}")]
    DocxGenerationError(String),

    /// An external renderer started but failed, timed out, or produced no
    /// usable image.
    #[error("external tool `{program}` failed: {detail}")]
    ExternalToolFailed {
        /// The program that failed.
        program: String,
        /// Exit status and the tool's own message, or why it was stopped.
        detail: String,
    },

    /// The host cancelled the conversion with a [`CancelToken`](crate::CancelToken).
    #[error("conversion cancelled: {0}")]
    Cancelled(String),

    /// The conversion ran past [`Limits::max_conversion_time`](crate::Limits).
    #[error("conversion timed out: {0}")]
    TimedOut(String),

    /// Reading an input file or an image referenced by the document failed.
    #[error("i/o error on {}: {source}", path.display())]
    Io {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

impl CrateError {
    /// True when the document itself is at fault (malformed, too big, or forbidden by the
    /// security policy): report it to whoever wrote the Markdown. False for problems on the
    /// operator's side (a missing tool, a bad [`Config`](crate::Config), I/O).
    ///
    /// [`CrateError::ExternalToolFailed`] is `false` because it is ambiguous: a renderer can
    /// reject a user's diagram text or be broken itself. Set
    /// [`GraphOptions::on_failure`](crate::GraphOptions) to
    /// [`DiagramFailure::KeepAsCode`](crate::DiagramFailure) to keep such blocks as code instead.
    pub fn is_input_error(&self) -> bool {
        matches!(
            self,
            CrateError::InvalidMarkdown(_) | CrateError::LimitExceeded(_) | CrateError::UnsafeContent(_)
        )
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CrateError::Io {
            path: path.into(),
            source,
        }
    }
}

/// `Result` with [`CrateError`] as the error type.
pub type Result<T> = std::result::Result<T, CrateError>;

/// How to get a tool that is commonly missing, appended to the error message.
pub(crate) fn install_hint(program: &str) -> &'static str {
    // Split on both separators so a Windows path is understood on any host.
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let name = base
        .strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".cmd"))
        .or_else(|| base.strip_suffix(".bat"))
        .unwrap_or(base);
    match name {
        "merman-cli" => "; install it with `cargo install merman-cli` (for your own documents, the `mmdc` engine is the alternative)",
        "mmdc" => "; install it with `npm install -g @mermaid-js/mermaid-cli`, or use the `merman` engine instead",
        "latex" | "dvipng" => {
            "; install a TeX distribution (TeX Live: texlive-latex-base and dvipng), or use native math"
        }
        _ => "; install it or point the configuration at it",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_messages_say_how_to_fix_it() {
        let msg = |p: &str| CrateError::MissingExternalCLI { program: p.into() }.to_string();
        assert!(msg("merman-cli").contains("cargo install merman-cli") && msg("merman-cli").contains("mmdc"));
        assert!(msg("/opt/bin/mmdc").contains("npm install -g @mermaid-js/mermaid-cli"));
        assert!(msg("latex").contains("native math"));
        assert!(msg("dolphin").contains("point the configuration at it"));
        assert!(msg("C:\\tools\\merman-cli.exe").contains("cargo install"));
    }
}
