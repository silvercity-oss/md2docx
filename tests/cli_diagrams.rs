//! `md2docx --untrusted --diagrams`.
#![cfg(feature = "cli")]

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::*;

fn md2docx(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .args(args)
        .output()
        .expect("md2docx should start")
}

fn p(path: &Path) -> &str {
    path.to_str().expect("utf-8 temp path")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

const DOC: &str = "# Doc\n\n```mermaid\ngraph TD; A-->B\n```\n\n```dolphin\nnode a\n```\n\n$$\\frac{a}{b}$$\n";

#[test]
fn diagrams_flag_needs_untrusted() {
    let fx = Fixture::new();
    let input = fx.path("d.md");
    std::fs::write(&input, DOC).unwrap();
    let out = md2docx(&[p(&input), "--diagrams"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("--untrusted"), "{}", stderr(&out));
    // It cannot be combined with switches that would loosen the preset either.
    let out = md2docx(&[p(&input), "--untrusted", "--diagrams", "--mermaid-engine", "mmdc"]);
    assert_eq!(out.status.code(), Some(2));
    let out = md2docx(&[
        p(&input),
        "--untrusted",
        "--diagrams",
        "--dolphin-command",
        "d -i {input} -o {output}",
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn untrusted_alone_still_starts_no_diagram_tool() {
    let fx = Fixture::new();
    let input = fx.path("d.md");
    std::fs::write(&input, DOC).unwrap();
    // Succeeds whether or not merman is installed, because nothing is run.
    let out = md2docx(&[p(&input), "--untrusted"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let docx = std::fs::read(fx.path("d.docx")).unwrap();
    assert_eq!(media_count(&docx), 0);
    assert!(visible_text(&zip_text(&docx, "word/document.xml")).contains("graph TD; A-->B"));
}

#[test]
fn untrusted_with_diagrams_renders_mermaid_or_says_how_to_install_merman() {
    let fx = Fixture::new();
    let input = fx.path("d.md");
    std::fs::write(&input, DOC).unwrap();
    let out = md2docx(&[p(&input), "--untrusted", "--diagrams"]);
    // A build with the `mermaid` feature renders in-process whether or not merman-cli exists.
    if cfg!(feature = "mermaid") || have("merman-cli") {
        assert!(out.status.success(), "{}", stderr(&out));
        let docx = std::fs::read(fx.path("d.docx")).unwrap();
        assert_eq!(media_count(&docx), 1, "the diagram");
        let body = zip_text(&docx, "word/document.xml");
        assert!(body.contains("<m:oMathPara>"));
        assert!(visible_text(&body).contains("node a"), "dolphin stays code");
    } else {
        assert_eq!(out.status.code(), Some(6));
        assert!(stderr(&out).contains("cargo install merman-cli"), "{}", stderr(&out));
        assert!(!fx.path("d.docx").exists(), "no partial output");
    }
}
