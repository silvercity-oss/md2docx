//! Tests against a real TeX installation. They skip when `latex`/`dvipng` are
//! missing, unless `REQUIRE_TEX=1` (set it in CI so a broken setup fails loudly).
#![cfg(unix)]

mod common;

use std::path::Path;
use std::time::Duration;

use common::*;
use md2docx::__internals::{run_tool, ToolGuard, TEX_ENV};
use md2docx::{markdown_to_docx, Config, CrateError};

fn tex_available() -> bool {
    let ok = have("latex") && have("dvipng");
    if !ok {
        assert!(
            std::env::var("REQUIRE_TEX").map_or(true, |v| v.is_empty()),
            "REQUIRE_TEX is set but latex/dvipng are missing"
        );
        eprintln!("skipping: latex/dvipng not installed");
    }
    ok
}

fn latex(dir: &Path, file: &str, env: &[(&str, &str)]) -> Result<(), CrateError> {
    let args = vec![
        "-interaction=nonstopmode".to_string(),
        "-halt-on-error".to_string(),
        "-no-shell-escape".to_string(),
        file.to_string(),
    ];
    run_tool(
        "latex",
        &args,
        dir,
        Duration::from_secs(60),
        &ToolGuard {
            env,
            max_dir_bytes: 64 << 20,
            overall_deadline: None,
            cancel: None,
            conversion_deadline: None,
            scrub_env_to: None,
        },
    )
}

#[test]
fn real_latex_renders_legitimate_formulas_through_the_whole_pipeline() {
    if !tex_available() {
        return;
    }
    let md = "Inline $\\alpha^2 + \\beta_i \\leq \\sqrt{x}$ text.\n\n$$\n\\int_0^1 \\frac{x^2}{1+x}\\,dx = \\sum_{k=1}^{n} \\binom{n}{k}\n$$\n\n\
              $$\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}$$\n";
    let docx = markdown_to_docx(md, &Config::trusted()).expect("default (image) mode must work with a real TeX");
    assert!(media_count(&docx) >= 3, "one picture per formula");
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(!body.contains("<m:oMath"));
}

#[test]
fn real_latex_reports_bad_math_cleanly() {
    if !tex_available() {
        return;
    }
    match markdown_to_docx("$\\undefinedcommandxyz$", &Config::trusted()) {
        Err(CrateError::ExternalToolFailed { program, detail }) => {
            assert_eq!(program, "latex");
            assert!(detail.contains("Undefined control sequence"), "{detail}");
        }
        other => panic!("expected a TeX error, got {:?}", other.map(|d| d.len())),
    }
}

/// The hardening that matters if the denylist were ever bypassed: even a formula that
/// reaches TeX cannot read or write outside its working directory.
#[test]
fn paranoid_mode_stops_tex_reading_and_writing_host_files() {
    if !tex_available() {
        return;
    }
    let host = tempfile::tempdir().unwrap();
    let secret = host.path().join("secret.txt");
    std::fs::write(&secret, "SECRET-TOKEN-12345\n").unwrap();
    let work = tempfile::tempdir().unwrap();

    // Reading an absolute path.
    std::fs::write(
        work.path().join("read.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}\\input{{{}}}\\end{{document}}",
            secret.display()
        ),
    )
    .unwrap();
    // Baseline: with only `-no-shell-escape` (what we shipped before) TeX does read it.
    assert!(
        latex(work.path(), "read.tex", &[]).is_ok(),
        "baseline: unhardened TeX reads host files"
    );
    let log = std::fs::read_to_string(work.path().join("read.log")).unwrap();
    assert!(log.contains("secret.txt"), "baseline should have opened the file");
    // With our environment it is refused.
    let _ = std::fs::remove_file(work.path().join("read.log"));
    match latex(work.path(), "read.tex", TEX_ENV) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("openin_any = p"), "{detail}");
            assert!(!detail.contains("SECRET-TOKEN"));
        }
        other => panic!("paranoid mode should refuse the read: {other:?}"),
    }

    // Reading via `..`.
    let rel = format!("../{}/secret.txt", host.path().file_name().unwrap().to_string_lossy());
    std::fs::write(
        work.path().join("rel.tex"),
        format!("\\documentclass{{article}}\\begin{{document}}\\input{{{rel}}}\\end{{document}}"),
    )
    .unwrap();
    assert!(latex(work.path(), "rel.tex", TEX_ENV).is_err());

    // Writing outside the working directory.
    let target = host.path().join("pwned.txt");
    std::fs::write(
        work.path().join("write.tex"),
        format!(
            "\\documentclass{{article}}\\begin{{document}}\\immediate\\openout3={}\\immediate\\write3{{owned}}\\end{{document}}",
            target.display()
        ),
    )
    .unwrap();
    assert!(latex(work.path(), "write.tex", TEX_ENV).is_err());
    assert!(!target.exists(), "TeX must not create files outside its directory");
}
