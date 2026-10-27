//! Links to files on the command line: `--allow-links`, `--link-*`, `--markdown-links`,
//! `--copy-linked`.

#![cfg(feature = "cli")]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;

fn run(args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_md2docx-cli"));
    c.args(args);
    for v in [
        "MD2DOCX_UNTRUSTED",
        "MD2DOCX_STRICT",
        "MD2DOCX_FORMAT",
        "MD2DOCX_QUIET",
        "SOURCE_DATE_EPOCH",
    ] {
        c.env_remove(v);
    }
    c.output().unwrap()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// project/{README.md, LICENSE, SECURITY.md, docs/{cli.md, deep/more.md}, tests/a.rs}
fn project() -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(guard.path()).unwrap().join("project");
    std::fs::create_dir_all(root.join("docs/deep")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    let w = |name: &str, text: &str| std::fs::write(root.join(name), text).unwrap();
    w(
        "README.md",
        "# Readme\n\nSee [the CLI](docs/cli.md#exit-status), the [licence](LICENSE), [security](SECURITY.md) and the [tests](tests/).\n",
    );
    w("LICENSE", "MIT\n");
    w("SECURITY.md", "# Security\n\nBack to the [readme](README.md).\n");
    w(
        "docs/cli.md",
        "# CLI\n\n## Exit status\n\nSee [more](deep/more.md) and [home](../README.md).\n",
    );
    w("docs/deep/more.md", "# More\n\n[up](../cli.md#cli)\n");
    w("tests/a.rs", "fn main() {}\n");
    (guard, root)
}

fn rels(docx_path: &Path) -> String {
    zip_text(&std::fs::read(docx_path).unwrap(), "word/_rels/document.xml.rels")
}

#[test]
fn local_links_are_off_unless_asked_for() {
    let (_g, root) = project();
    let readme = root.join("README.md");
    let out = run(&["-q", p(&readme), "-o", "-"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!zip_text(&out.stdout, "word/_rels/document.xml.rels").contains("LICENSE"));
}

#[test]
fn allow_links_local_rebases_to_the_output_directory() {
    let (g, root) = project();
    let build = g.path().join("build");
    std::fs::create_dir(&build).unwrap();
    let out_file = build.join("README.docx");
    let out = run(&[p(&root.join("README.md")), "--allow-links", "local", "-o", p(&out_file)]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    let r = rels(&out_file);
    assert!(r.contains("Target=\"../project/LICENSE\""), "{r}");
    assert!(r.contains("Target=\"../project/docs/cli.md\""), "{r}");
    assert!(r.contains("Target=\"../project/tests/\""), "{r}");
    assert!(!err(&out).contains("warning"), "{}", err(&out));

    let abs = run(&[
        "-q",
        p(&root.join("README.md")),
        "--allow-links",
        "local",
        "--link-style",
        "absolute",
        "-o",
        "-",
    ]);
    let r = zip_text(&abs.stdout, "word/_rels/document.xml.rels");
    assert!(r.contains(&format!("Target=\"{}/LICENSE\"", file_url(&root))), "{r}");
}

#[test]
fn broken_links_are_findings_and_strict_check_makes_them_a_failure() {
    let (_g, root) = project();
    std::fs::write(
        root.join("bad.md"),
        "[gone](nope.md) [anchor](docs/cli.md#no-such-heading) [ok](LICENSE)\n",
    )
    .unwrap();
    let out = run(&["--check", "--strict", p(&root.join("bad.md")), "--allow-links", "local"]);
    assert_eq!(out.status.code(), Some(5), "{}", err(&out));
    let e = err(&out);
    assert!(
        e.contains("link.target-missing") && e.contains("link.anchor-missing"),
        "{e}"
    );
    // Without --strict they are warnings.
    let lenient = run(&["--check", p(&root.join("bad.md")), "--allow-links", "local"]);
    assert_eq!(lenient.status.code(), Some(0));
}

#[test]
fn link_options_need_local_links_and_do_not_combine_with_untrusted() {
    let (_g, root) = project();
    let readme = p(&root.join("README.md")).to_string();
    for args in [
        vec![readme.as_str(), "--link-base", "https://example.com/"],
        vec![readme.as_str(), "--link-root", "."],
        vec![readme.as_str(), "--untrusted", "--allow-links", "local"],
        vec![readme.as_str(), "--untrusted", "--markdown-links", "follow"],
        vec![readme.as_str(), "--copy-linked"],
        vec![
            readme.as_str(),
            "--allow-links",
            "local",
            "--link-map",
            "no-equals-sign",
        ],
    ] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", err(&out));
    }
}

#[test]
fn local_files_can_become_web_links() {
    let (_g, root) = project();
    let out = run(&[
        "-q",
        p(&root.join("README.md")),
        "-o",
        "-",
        "--allow-links",
        "local",
        "--link-base",
        "https://github.com/me/repo/blob/v1/",
        "--link-tree-base",
        "https://github.com/me/repo/tree/v1/",
        "--link-map",
        "docs/=https://docs.example.com/",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    let r = zip_text(&out.stdout, "word/_rels/document.xml.rels");
    for want in [
        "https://docs.example.com/cli.md#exit-status",
        "https://github.com/me/repo/blob/v1/LICENSE",
        "https://github.com/me/repo/tree/v1/tests/",
    ] {
        assert!(r.contains(want), "{want} not in {r}");
    }
}

#[test]
fn follow_converts_every_linked_markdown_file_into_a_mirrored_tree() {
    let (g, root) = project();
    let site = g.path().join("site");
    let out = run(&[
        "--format",
        "json",
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "-o",
        p(&site),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    for f in ["README.docx", "SECURITY.docx", "docs/cli.docx", "docs/deep/more.docx"] {
        assert!(site.join(f).exists(), "{f} missing");
    }
    // Cycles (README <-> SECURITY, cli <-> more) do not repeat work.
    let lines: Vec<String> = err(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.contains("\"origin\":\"link\"")).count(), 3);
    assert_eq!(lines.iter().filter(|l| l.contains("\"origin\":\"input\"")).count(), 1);
    // Links point at the documents, and into them by heading.
    let readme = rels(&site.join("README.docx"));
    assert!(readme.contains("Target=\"docs/cli.docx\""), "{readme}");
    assert!(readme.contains("Target=\"SECURITY.docx\""), "{readme}");
    let more = rels(&site.join("docs/deep/more.docx"));
    assert!(more.contains("Target=\"../cli.docx\""), "{more}");
    let cli = rels(&site.join("docs/cli.docx"));
    assert!(
        cli.contains("Target=\"deep/more.docx\"") && cli.contains("Target=\"../README.docx\""),
        "{cli}"
    );
    // Without --copy-linked the other files stay where they are, relative to the output.
    assert!(readme.contains("LICENSE") && readme.contains("../project/"), "{readme}");
    assert!(!site.join("LICENSE").exists());
}

#[test]
fn copy_linked_brings_the_other_files_and_directories_along() {
    let (g, root) = project();
    let site = g.path().join("site");
    let out = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "--copy-linked",
        "-o",
        p(&site),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert_eq!(std::fs::read_to_string(site.join("LICENSE")).unwrap(), "MIT\n");
    assert!(site.join("tests/a.rs").exists());
    let readme = rels(&site.join("README.docx"));
    assert!(
        readme.contains("Target=\"LICENSE\"") && readme.contains("Target=\"tests/\""),
        "{readme}"
    );
}

#[test]
fn the_linked_file_and_copy_limits_stop_the_run() {
    let (g, root) = project();
    let site = g.path().join("site");
    let many = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "--max-linked-files",
        "2",
        "-o",
        p(&site),
    ]);
    assert_eq!(many.status.code(), Some(4), "{}", err(&many));
    assert!(err(&many).contains("--max-linked-files"), "{}", err(&many));
    let big = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "--copy-linked",
        "--max-copy-bytes",
        "3",
        "-o",
        p(&g.path().join("site2")),
    ]);
    assert_eq!(big.status.code(), Some(4), "{}", err(&big));
}

#[test]
fn docx_mode_rewrites_links_but_converts_only_the_named_files() {
    let (g, root) = project();
    let site = g.path().join("site");
    let out = run(&[
        p(&root.join("README.md")),
        p(&root.join("docs/cli.md")),
        "--markdown-links",
        "docx",
        "-o",
        p(&site),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(site.join("README.docx").exists() && site.join("docs/cli.docx").exists());
    assert!(!site.join("SECURITY.docx").exists() && !site.join("docs/deep/more.docx").exists());
    assert!(rels(&site.join("README.docx")).contains("Target=\"SECURITY.docx\""));
}

#[test]
fn a_flat_layout_puts_everything_in_one_directory_and_refuses_clashes() {
    let (g, root) = project();
    let site = g.path().join("flat");
    std::fs::write(root.join("docs/README.md"), "# Other readme\n").unwrap();
    std::fs::write(root.join("docs/cli.md"), "# CLI\n\n[x](deep/more.md)\n").unwrap();
    let ok = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "--link-layout",
        "flat",
        "-o",
        p(&site),
    ]);
    assert_eq!(ok.status.code(), Some(0), "{}", err(&ok));
    for f in ["README.docx", "SECURITY.docx", "cli.docx", "more.docx"] {
        assert!(site.join(f).exists(), "{f}");
    }
    // Two different sources with one name cannot share a flat directory.
    let clash = run(&[
        p(&root.join("README.md")),
        p(&root.join("docs/README.md")),
        "--markdown-links",
        "docx",
        "--link-layout",
        "flat",
        "-o",
        p(&g.path().join("flat2")),
    ]);
    assert_eq!(clash.status.code(), Some(2), "{}", err(&clash));
}

#[test]
fn documents_need_files_and_a_directory_for_output() {
    let (g, root) = project();
    let stdin = run(&["-", "--markdown-links", "docx"]);
    assert_eq!(stdin.status.code(), Some(2));
    let file_out = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "docx",
        "-o",
        p(&root.join("LICENSE")),
    ]);
    assert_eq!(file_out.status.code(), Some(2), "{}", err(&file_out));
    let in_place_copy = run(&[
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "--copy-linked",
    ]);
    assert_eq!(in_place_copy.status.code(), Some(2), "{}", err(&in_place_copy));
    let _ = g;
}

#[test]
fn follow_with_check_writes_nothing_but_reports_every_document() {
    let (g, root) = project();
    let site = g.path().join("never");
    let out = run(&[
        "--check",
        "--format",
        "json",
        p(&root.join("README.md")),
        "--markdown-links",
        "follow",
        "-o",
        p(&site),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(err(&out).lines().count(), 4);
    assert!(!site.exists());
}

#[test]
fn in_place_follow_writes_each_docx_next_to_its_markdown() {
    let (_g, root) = project();
    let out = run(&["-q", p(&root.join("README.md")), "--markdown-links", "follow"]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(root.join("README.docx").exists() && root.join("docs/deep/more.docx").exists());
    assert!(rels(&root.join("README.docx")).contains("Target=\"docs/cli.docx\""));
}
