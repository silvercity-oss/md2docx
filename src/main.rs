//! `md2docx-cli`: command-line front end for the Markdown -> .docx converter.

mod cli;

fn main() -> std::process::ExitCode {
    cli::main()
}
