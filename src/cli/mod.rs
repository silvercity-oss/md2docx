//! `md2docx-cli`: command-line front end for the Markdown -> .docx converter.
//!
//! The interface (flags, exit status, the JSON report) is documented in `CLI.md`.

/// Write a line to standard error, ignoring failure: a closed or full stderr must not turn a
/// finished conversion into a panic (`eprintln!` panics when it cannot write).
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($t)*);
    }};
}

mod args;
mod exit;
mod report;
mod run;
mod settings;
mod tools;

use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use args::{Args, Format};
use exit::{CliError, Exit};

pub fn main() -> ExitCode {
    let args = Args::parse();
    if args.completions.is_some() || args.man {
        return ExitCode::from(print_support_text(&args));
    }
    if let Some(code) = tools::dispatch(&args) {
        return ExitCode::from(code);
    }
    let json = args.format == Format::Json;
    if json {
        // The report is the only thing on standard error.
        std::panic::set_hook(Box::new(|_| {}));
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| match settings::config(&args) {
        Ok(cfg) => run::run(&args, &cfg),
        Err(e) => run::fatal(&args, &e),
    }));
    match outcome {
        Ok(code) => ExitCode::from(code),
        Err(payload) => {
            let what = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            let e = CliError::internal(format!("internal error (this is a bug in md2docx-cli): {what}"));
            ExitCode::from(run::fatal(&args, &e))
        }
    }
}

/// `--completions SHELL` and `--man`: write to standard output and stop.
fn print_support_text(args: &Args) -> u8 {
    let mut cmd = Args::command();
    let mut out = std::io::stdout().lock();
    if let Some(shell) = args.completions {
        clap_complete::generate(shell, &mut cmd, "md2docx-cli", &mut out);
    } else if clap_mangen::Man::new(cmd).render(&mut out).is_err() {
        return Exit::Io.code();
    }
    if out.flush().is_err() {
        return Exit::Io.code();
    }
    Exit::Ok.code()
}
