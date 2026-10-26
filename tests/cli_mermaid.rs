//! The in-process Mermaid renderer from the command line (builds with the `mermaid` feature).
//! With an empty `PATH` nothing could be started, so a picture proves nothing was.

#![cfg(all(feature = "cli", feature = "mermaid"))]

mod common;

use std::process::{Command, Output};

use common::*;

const DOC: &str = "# Flow\n\n```mermaid\ngraph TD; A[Start] --> B[End]\n```\n";

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .args(args)
        .env("PATH", "")
        .output()
        .unwrap()
}

#[test]
fn the_inprocess_engine_renders_without_starting_a_process() {
    let fx = Fixture::new();
    let input = fx.path("flow.md");
    std::fs::write(&input, DOC).unwrap();
    let p = input.to_str().unwrap();
    let out = run(&["-q", p, "-o", "-", "--mermaid-engine", "inprocess"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(media_count(&out.stdout), 1, "the diagram");
}

#[test]
fn untrusted_diagrams_use_the_inprocess_renderer_in_this_build() {
    let fx = Fixture::new();
    let input = fx.path("flow.md");
    std::fs::write(&input, DOC).unwrap();
    let out = run(&["-q", input.to_str().unwrap(), "-o", "-", "--untrusted", "--diagrams"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(media_count(&out.stdout), 1);
}

#[test]
fn the_long_version_says_the_feature_is_compiled_in() {
    let out = run(&["--version"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("mermaid (in-process Mermaid available)"));
}
