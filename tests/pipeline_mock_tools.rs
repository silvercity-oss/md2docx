//! Pipeline tests that drive external tools.
//!
//! The container running CI has no `mmdc`, `latex` or `dvipng`, so these tests
//! put small stand-in executables (POSIX shell scripts) in a temp directory.
//! They behave like the real tools as far as the crate can tell: read the
//! source file, write a valid PNG to the requested path, and log what they saw.
//! Shell scripts make this file Unix-only; everything that does not need a
//! tool lives in `pipeline.rs` and runs on every platform.
#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use common::*;
use silvercity_oss::{markdown_to_docx, preprocess, AssetKind, CommandSpec, Config, CrateError, MathImageEngine};

trait MockTools {
    fn script(&self, name: &str, body: &str) -> String;
    fn fake_renderer(&self, name: &str) -> String;
    fn log(&self, name: &str) -> String;
    fn config(&self) -> Config;
}

impl MockTools for Fixture {
    fn script(&self, name: &str, body: &str) -> String {
        let p = self.path(name);
        fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    /// A fake `-i IN -o OUT` renderer: appends IN's text and OUT's path to
    /// `<name>.log`, then copies the fixture PNG to OUT.
    fn fake_renderer(&self, name: &str) -> String {
        let log = self.path(&format!("{name}.log"));
        let png = self.path("fixture.png");
        self.script(
            name,
            &format!(
                r#"while [ $# -gt 0 ]; do
  case "$1" in -i) IN="$2"; shift 2;; -o) OUT="$2"; shift 2;; *) shift;; esac
done
{{ echo "OUT=$OUT"; cat "$IN"; echo "---"; }} >> '{log}'
cp '{png}' "$OUT""#,
                log = log.display(),
                png = png.display()
            ),
        )
    }

    fn log(&self, name: &str) -> String {
        fs::read_to_string(self.path(&format!("{name}.log"))).unwrap_or_default()
    }

    fn config(&self) -> Config {
        let mut cfg = Config::default();
        cfg.graphs.mermaid = CommandSpec::new(self.fake_renderer("mmdc"), ["-i", "{input}", "-o", "{output}"]);
        cfg.graphs.dolphin = CommandSpec::new(self.fake_renderer("dolphin"), ["-i", "{input}", "-o", "{output}"]);
        cfg.math.engine = MathImageEngine::Command(CommandSpec::new(
            self.fake_renderer("mathtool"),
            ["-i", "{input}", "-o", "{output}", "--display={display}", "--dpi={dpi}"],
        ));
        cfg
    }
}

fn recorded_outputs(log: &str) -> Vec<PathBuf> {
    log.lines()
        .filter_map(|l| l.strip_prefix("OUT="))
        .map(PathBuf::from)
        .collect()
}

#[test]
fn mermaid_block_is_rendered_and_embedded() {
    let fx = Fixture::new();
    let md = "# Flow\n\nBefore.\n\n```mermaid\ngraph TD;\n  A-->B;\n```\n\nAfter.\n";
    let docx = markdown_to_docx(md, &fx.config()).unwrap();

    // The renderer received exactly the fence's payload.
    let log = fx.log("mmdc");
    assert!(log.contains("graph TD;\n  A-->B;\n"), "renderer saw: {log}");

    // The PNG is embedded in the package and referenced from the body.
    assert_eq!(media_count(&docx), 1);
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(body.contains("<w:drawing>"));
    assert!(
        !body.contains("A--&gt;B"),
        "the diagram source must not leak into the text"
    );
    for t in ["Flow", "Before.", "After."] {
        assert!(body.contains(t));
    }
}

#[test]
fn preprocessor_rewrites_fences_into_image_syntax() {
    let fx = Fixture::new();
    let md = "Intro\n\n```mermaid\ngraph LR; A-->B\n```\n\n```dolphin\nnode a\n```\n\n```rust\nlet x = 1;\n```\n";
    let pre = preprocess(md, &fx.config()).unwrap();

    assert_eq!(
        pre.markdown.matches("![Rendered Graph](<").count(),
        2,
        "{}",
        pre.markdown
    );
    assert!(pre.markdown.contains("graph_") && pre.markdown.contains(".png"));
    assert!(!pre.markdown.contains("mermaid") && !pre.markdown.contains("dolphin"));
    assert!(
        pre.markdown.contains("```rust\nlet x = 1;\n```"),
        "other fences are untouched"
    );
    assert!(
        fx.log("dolphin").contains("node a"),
        "dolphin blocks use the dolphin renderer"
    );

    // Every asset points at a real PNG inside a workspace that dies with `pre`.
    let pngs: Vec<PathBuf> = pre
        .assets
        .values()
        .map(|a| match &a.source {
            silvercity_oss::AssetSource::Png(p) => p.clone(),
            other => panic!("expected PNG, got {other:?}"),
        })
        .collect();
    assert_eq!(pngs.len(), 2);
    assert!(pre.assets.values().all(|a| a.kind == AssetKind::Graph));
    assert!(pngs.iter().all(|p| p.exists()));
    let workspace = pngs[0].parent().unwrap().to_path_buf();
    drop(pre);
    assert!(
        !workspace.exists(),
        "workspace must be wiped when the preprocessed document is dropped"
    );
}

#[test]
fn identical_diagrams_are_rendered_once() {
    let fx = Fixture::new();
    let md = "```mermaid\ngraph TD; A-->B\n```\n\ntext\n\n```mermaid\ngraph TD; A-->B\n```\n";
    markdown_to_docx(md, &fx.config()).unwrap();
    assert_eq!(recorded_outputs(&fx.log("mmdc")).len(), 1);
}

#[test]
fn graph_inside_blockquote_and_list_is_replaced() {
    let fx = Fixture::new();
    let md = "> ```mermaid\n> graph TD; A-->B\n> ```\n\n- item\n\n  ```dolphin\n  node a\n  ```\n";
    let pre = preprocess(md, &fx.config()).unwrap();
    assert_eq!(pre.assets.len(), 2, "{}", pre.markdown);
    assert!(
        fx.log("mmdc").contains("graph TD; A-->B"),
        "quote prefix must not reach the renderer"
    );
    assert!(!fx.log("mmdc").contains("> "));
    assert!(pre.markdown.starts_with("> ![Rendered Graph]("), "{}", pre.markdown);
}

#[test]
fn math_image_fallback_renders_inline_and_display_formulas() {
    let fx = Fixture::new();
    let md = "Euler: $e^{i\\pi} + 1 = 0$ inline.\n\n$$\n\\int_0^1 x^2\\,dx = \\frac13\n$$\n";
    let docx = markdown_to_docx(md, &fx.config()).unwrap();

    let log = fx.log("mathtool");
    assert!(log.contains("e^{i\\pi} + 1 = 0"), "{log}");
    assert!(log.contains("\\int_0^1 x^2\\,dx = \\frac13"), "{log}");
    assert_eq!(recorded_outputs(&log).len(), 2);

    // Both formulas reuse one fixture PNG; docx-rs stores identical bytes once.
    assert_eq!(media_count(&docx), 1);
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert_eq!(body.matches("<w:drawing>").count(), 2, "one picture per formula");
    assert!(!body.contains("<m:oMath"), "image mode must not emit OMML");
    assert!(!body.contains("frac13"), "LaTeX source must not leak into the text");
    assert!(body.contains("Euler:") && body.contains("inline."));
    assert!(body.contains(r#"<w:jc w:val="center"/>"#), "display formula is centred");
}

#[test]
fn display_flag_and_dpi_reach_the_math_tool() {
    let fx = Fixture::new();
    let mut cfg = fx.config();
    let echo = fx.script(
        "flags",
        &format!(
            "echo \"$@\" >> '{}'\ncp '{}' \"$3\"",
            fx.path("flags.log").display(),
            fx.path("fixture.png").display()
        ),
    );
    cfg.math.dpi = 450;
    cfg.math.engine = MathImageEngine::Command(CommandSpec::new(
        echo,
        ["{input}", "--flags", "{output}", "d={display}", "r={dpi}"],
    ));
    // `$3` above is {output}.
    markdown_to_docx("inline $a$ and\n\n$$b$$\n", &cfg).unwrap();
    let log = fx.log("flags");
    assert!(log.contains("d=0 r=450"), "{log}");
    assert!(log.contains("d=1 r=450"), "{log}");
}

#[test]
fn latex_dvipng_engine_builds_a_document_and_converts_it() {
    let fx = Fixture::new();
    let latex = fx.script(
        "latex",
        &format!(
            "for last; do :; done\ncp \"$last\" '{log}'\nbase=\"${{last%.tex}}\"\n: > \"$base.dvi\"",
            log = fx.path("latex.seen").display()
        ),
    );
    let dvipng = fx.script(
        "dvipng",
        &format!(
            "echo \"$@\" > '{log}'\nwhile [ $# -gt 0 ]; do case \"$1\" in -o) OUT=\"$2\"; shift 2;; *) shift;; esac; done\ncp '{png}' \"$OUT\"",
            log = fx.path("dvipng.seen").display(),
            png = fx.path("fixture.png").display()
        ),
    );
    let mut cfg = Config::default();
    cfg.math.engine = MathImageEngine::LatexDvipng { latex, dvipng };
    let docx = markdown_to_docx("$$\n\\sum_{k=1}^n k\n$$\n", &cfg).unwrap();

    let tex = fs::read_to_string(fx.path("latex.seen")).unwrap();
    assert!(tex.contains("\\usepackage{amsmath,amssymb}"));
    assert!(tex.contains("\\[\n\\sum_{k=1}^n k\n\\]"), "{tex}");
    let args = fs::read_to_string(fx.path("dvipng.seen")).unwrap();
    assert!(
        args.contains("-bg Transparent") && args.contains("-T tight") && args.contains("-D 300"),
        "{args}"
    );
    assert_eq!(media_count(&docx), 1);
}

#[test]
fn failing_tool_surfaces_its_stderr() {
    let fx = Fixture::new();
    let mut cfg = fx.config();
    cfg.graphs.mermaid = CommandSpec::new(
        fx.script("boom", "echo 'Parse error on line 2' >&2\nexit 3"),
        Vec::<String>::new(),
    );
    match markdown_to_docx("```mermaid\ngraph TD; A-->\n```\n", &cfg) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("Parse error on line 2"), "{detail}")
        }
        other => panic!("expected ExternalToolFailed, got {other:?}"),
    }
}

#[test]
fn tool_that_writes_no_png_is_an_error() {
    let fx = Fixture::new();
    let mut cfg = fx.config();
    cfg.graphs.mermaid = CommandSpec::new(fx.script("liar", "echo not-a-png > \"$2\""), ["-o", "{output}"]);
    cfg.graphs.mermaid.args = vec!["{input}".into(), "{output}".into()];
    assert!(matches!(
        markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg),
        Err(CrateError::ExternalToolFailed { .. })
    ));
}

#[test]
fn hung_tool_is_killed_after_timeout() {
    let fx = Fixture::new();
    let mut cfg = fx.config();
    cfg.tool_timeout = Duration::from_millis(300);
    cfg.graphs.mermaid = CommandSpec::new(fx.script("hang", "sleep 30"), Vec::<String>::new());
    let started = std::time::Instant::now();
    match markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => assert!(detail.contains("timed out"), "{detail}"),
        other => panic!("expected timeout, got {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn temp_files_are_removed_even_when_conversion_fails() {
    let fx = Fixture::new();
    // First diagram renders (and leaves a PNG in the workspace), second one fails.
    let mut cfg = fx.config();
    let dolphin = fx.script("dolphin-fail", "echo bad >&2\nexit 1");
    cfg.graphs.dolphin = CommandSpec::new(dolphin, ["{input}", "{output}"]);
    let md = "```mermaid\ngraph TD; A-->B\n```\n\n```dolphin\nnode\n```\n";
    assert!(markdown_to_docx(md, &cfg).is_err());
    for png in recorded_outputs(&fx.log("mmdc")) {
        assert!(!png.exists(), "{} leaked", png.display());
    }
}
