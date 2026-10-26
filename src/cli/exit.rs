//! The exit-status contract of `md2docx`. See `docs/cli.md`: these values are part of the
//! public interface and do not change meaning once released.

use md2docx::{CrateError, Diagnostic};

/// What a script is told. Chosen by *who has to act*, not by which internal stage failed, so a
/// wrapper can decide between "tell the author", "page the operator" and "retry" with a `case`
/// on `$?`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// The document was converted (or, with `--check`, found convertible). Warnings may have
    /// been reported.
    Ok = 0,
    /// Anything not covered below, including a bug in this program. Also the status when a
    /// conversion was cancelled.
    Failure = 1,
    /// The command line or its configuration is wrong: unknown flag, conflicting options, a limit
    /// that `--untrusted` forbids raising, an unreadable `--reference-doc`. (`clap` also uses 2.)
    /// Nothing was converted.
    Usage = 2,
    /// A file could not be read or written: the input, the output, the output directory, the
    /// standard streams. The document was not judged.
    Io = 3,
    /// The *document* is at fault: malformed, over a limit, or forbidden by the security policy.
    /// Retrying the same input gives the same answer; tell whoever wrote it.
    Rejected = 4,
    /// `--strict`: the document is valid but could not be converted without loss, so nothing was
    /// written. The findings were reported.
    Lossy = 5,
    /// An external program is missing or failed (`latex`, `dvipng`, `mmdc`, `merman-cli`, a
    /// custom renderer). The environment is at fault, unless the renderer rejected a diagram, in
    /// which case `--keep-failed-diagrams` keeps it as code.
    Tool = 6,
    /// `--deadline` passed. A retry with more time, or on a less loaded machine, may succeed.
    Timeout = 7,
}

impl Exit {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// A failure, with the status to exit with and a stable machine-readable `kind` (the `error.kind`
/// of the JSON report).
#[derive(Debug)]
pub struct CliError {
    pub exit: Exit,
    pub kind: &'static str,
    pub message: String,
    /// The findings behind a `--strict` refusal.
    pub diagnostics: Vec<Diagnostic>,
}

impl CliError {
    pub fn new(exit: Exit, kind: &'static str, message: impl Into<String>) -> Self {
        CliError {
            exit,
            kind,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// A copy, for reporting the same failure twice.
    pub fn clone_error(&self) -> Self {
        CliError {
            exit: self.exit,
            kind: self.kind,
            message: self.message.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(Exit::Usage, "usage", message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(Exit::Io, "io", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Exit::Failure, "internal", message)
    }
}

impl From<CrateError> for CliError {
    fn from(e: CrateError) -> Self {
        let message = match &e {
            CrateError::LossyContent { .. } => {
                "the document cannot be converted faithfully (--strict); nothing was written".to_string()
            }
            other => other.to_string(),
        };
        let (exit, kind) = match &e {
            CrateError::InvalidMarkdown(_) => (Exit::Rejected, "invalid_markdown"),
            CrateError::LimitExceeded(_) => (Exit::Rejected, "limit_exceeded"),
            CrateError::UnsafeContent(_) => (Exit::Rejected, "unsafe_content"),
            CrateError::InvalidConfig(_) => (Exit::Usage, "invalid_config"),
            CrateError::MissingExternalCli { .. } => (Exit::Tool, "tool_missing"),
            CrateError::ExternalToolFailed { .. } => (Exit::Tool, "tool_failed"),
            CrateError::LossyContent { .. } => (Exit::Lossy, "lossy_content"),
            CrateError::TimedOut(_) => (Exit::Timeout, "timed_out"),
            CrateError::Cancelled(_) => (Exit::Failure, "cancelled"),
            CrateError::Io { .. } => (Exit::Io, "io"),
            CrateError::DocxGeneration(_) => (Exit::Failure, "internal"),
            _ => (Exit::Failure, "internal"),
        };
        let diagnostics = match e {
            CrateError::LossyContent { diagnostics } => diagnostics,
            _ => Vec::new(),
        };
        CliError {
            exit,
            kind,
            message,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_the_documented_ones() {
        // Changing any of these is a breaking change of the command-line interface.
        let all = [
            (Exit::Ok, 0),
            (Exit::Failure, 1),
            (Exit::Usage, 2),
            (Exit::Io, 3),
            (Exit::Rejected, 4),
            (Exit::Lossy, 5),
            (Exit::Tool, 6),
            (Exit::Timeout, 7),
        ];
        for (exit, n) in all {
            assert_eq!(exit.code(), n, "{exit:?}");
        }
    }

    #[test]
    fn every_library_error_has_a_class() {
        let cases: Vec<(CrateError, Exit, &str)> = vec![
            (
                CrateError::InvalidMarkdown("x".into()),
                Exit::Rejected,
                "invalid_markdown",
            ),
            (CrateError::LimitExceeded("x".into()), Exit::Rejected, "limit_exceeded"),
            (CrateError::UnsafeContent("x".into()), Exit::Rejected, "unsafe_content"),
            (CrateError::InvalidConfig("x".into()), Exit::Usage, "invalid_config"),
            (
                CrateError::MissingExternalCli { program: "p".into() },
                Exit::Tool,
                "tool_missing",
            ),
            (
                CrateError::ExternalToolFailed {
                    program: "p".into(),
                    detail: "d".into(),
                },
                Exit::Tool,
                "tool_failed",
            ),
            (
                CrateError::LossyContent { diagnostics: vec![] },
                Exit::Lossy,
                "lossy_content",
            ),
            (CrateError::TimedOut("x".into()), Exit::Timeout, "timed_out"),
            (CrateError::Cancelled("x".into()), Exit::Failure, "cancelled"),
            (CrateError::DocxGeneration("x".into()), Exit::Failure, "internal"),
            (
                CrateError::Io {
                    path: "p".into(),
                    source: std::io::Error::other("x"),
                },
                Exit::Io,
                "io",
            ),
        ];
        for (error, exit, kind) in cases {
            let label = format!("{error:?}");
            let got = CliError::from(error);
            assert_eq!((got.exit, got.kind), (exit, kind), "{label}");
        }
    }
}
