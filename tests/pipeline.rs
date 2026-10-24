//! End-to-end tests for the Markdown -> .docx pipeline.
//!
//! The container running CI has no `mmdc`, `latex` or `dvipng`, so the tests
//! put small stand-in executables (POSIX shell scripts) in a temp directory.
//! They behave like the real tools as far as the crate can tell: read the
//! source file, write a valid PNG to the requested path, and log what they saw.
#![cfg(unix)]

use std::fs;
use std::io::{Cursor, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use reqmech_companion::{markdown_to_docx, preprocess, AssetKind, CommandSpec, Config, CrateError, MathImageEngine};

// ---------------------------------------------------------------- helpers

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(600, 300)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        fs::write(dir.path().join("fixture.png"), png.into_inner()).unwrap();
        Fixture { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

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

fn zip_entries(docx: &[u8]) -> Vec<String> {
    let z = zip::ZipArchive::new(Cursor::new(docx)).expect("output must be a zip archive");
    z.file_names().map(str::to_string).collect()
}

fn zip_text(docx: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(Cursor::new(docx)).unwrap();
    let mut s = String::new();
    z.by_name(name)
        .unwrap_or_else(|_| panic!("{name} missing"))
        .read_to_string(&mut s)
        .unwrap();
    // docx-rs writes empty elements as `<w:b />`; normalise so asserts can use `<w:b/>`.
    s.replace(" />", "/>")
}

fn media_count(docx: &[u8]) -> usize {
    zip_entries(docx)
        .iter()
        .filter(|n| n.starts_with("word/media/") && !n.ends_with('/'))
        .count()
}

/// Assert the XML part is well-formed (balanced tags, valid escapes).
fn assert_well_formed(xml: &str) {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_str(xml);
    r.config_mut().check_end_names = true;
    loop {
        match r.read_event() {
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("document.xml is not well-formed: {e}"),
        }
    }
}

fn recorded_outputs(log: &str) -> Vec<PathBuf> {
    log.lines()
        .filter_map(|l| l.strip_prefix("OUT="))
        .map(PathBuf::from)
        .collect()
}

// ------------------------------------------------------ graph interception

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
            reqmech_companion::AssetSource::Png(p) => p.clone(),
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

// ---------------------------------------------------- math: image fallback

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

// -------------------------------------------------- math: native OMML mode

#[test]
fn native_omml_mode_injects_editable_equations_without_external_tools() {
    // No renderer is configured anywhere: this mode must not spawn anything.
    let mut cfg = Config::native_math();
    cfg.math.engine = MathImageEngine::Command(CommandSpec::new("/nonexistent/never-run", Vec::<String>::new()));
    cfg.graphs.mermaid.program = "/nonexistent/never-run".into();

    let md = "Inline $x_i^2 \\leq \\alpha$ here.\n\n$$\n\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}\n$$\n\nTail.\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();

    assert_eq!(media_count(&docx), 0, "native math embeds no pictures");
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);

    // Display equation: a real <m:oMathPara> paragraph with a fraction and a root.
    assert!(body.contains("<m:oMathPara>"), "{body}");
    assert!(body.contains("<m:f>") && body.contains("<m:rad>"));
    assert!(body.contains("<m:num>") && body.contains("<m:den>"));
    // Inline equation: <m:oMath> directly inside the text paragraph.
    assert!(body.contains("<m:sSubSup>"));
    assert!(body.contains("α") && body.contains("≤"));
    // Placeholders are gone and surrounding text survives.
    assert!(!body.contains("REQMECHOMML"));
    assert!(body.contains("Inline ") && body.contains(" here.") && body.contains("Tail."));
    // The two forms: exactly one display paragraph, two equations in total.
    assert_eq!(body.matches("<m:oMathPara>").count(), 1);
    assert_eq!(body.matches("<m:oMath>").count(), 2);
    assert_eq!(
        body.matches("xmlns:m=").count(),
        1,
        "math namespace is declared once, on the root"
    );
}

#[test]
fn omml_equation_inside_table_list_and_heading_survives() {
    let cfg = Config::native_math();
    let md = "# Title $E=mc^2$\n\n- item $a+b$\n\n| h | v |\n|---|---|\n| $$\\pi$$ | $c^2$ |\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(!body.contains("REQMECHOMML"));
    assert_eq!(body.matches("<m:oMathPara>").count(), 1, "solo display cell paragraph");
    assert_eq!(
        body.matches("<m:oMath>").count(),
        4,
        "heading, list item, and two table cells"
    );
}

#[test]
fn dollars_in_code_and_prose_are_not_math() {
    let cfg = Config::native_math();
    let md = "Costs $5 and $6 today.\n\nCode `$x$` stays, escaped \\$y\\$ too.\n\n```text\n$$ z $$\n```\n";
    let pre = preprocess(md, &cfg).unwrap();
    assert!(pre.assets.is_empty(), "{:?}", pre.markdown);
    assert_eq!(pre.markdown, md);
    let docx = markdown_to_docx(md, &cfg).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert!(body.contains("$x$") && body.contains("$$ z $$") && body.contains("$5 and $6"));
    assert!(!body.contains("<m:oMath"));
}

// ------------------------------------------------------------------ errors

#[test]
fn missing_cli_is_reported_by_name() {
    let mut cfg = Config::default();
    cfg.graphs.mermaid.program = "definitely-not-installed-mmdc".into();
    match markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg) {
        Err(CrateError::MissingExternalCLI { program }) => assert_eq!(program, "definitely-not-installed-mmdc"),
        other => panic!("expected MissingExternalCLI, got {other:?}"),
    }
    let mut cfg = Config::default();
    cfg.math.engine = MathImageEngine::LatexDvipng {
        latex: "no-such-latex".into(),
        dvipng: "no-such-dvipng".into(),
    };
    assert!(matches!(
        markdown_to_docx("$x$", &cfg),
        Err(CrateError::MissingExternalCLI { program }) if program == "no-such-latex"
    ));
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
fn invalid_input_is_rejected_not_panicked_on() {
    let fx = Fixture::new();
    assert!(matches!(
        markdown_to_docx("```mermaid\n   \n```\n", &fx.config()),
        Err(CrateError::InvalidMarkdown(m)) if m.contains("line 1")
    ));
    assert!(matches!(
        markdown_to_docx("a\0b", &fx.config()),
        Err(CrateError::InvalidMarkdown(_))
    ));
    let err = markdown_to_docx("ok\n\n$$\\left( x$$\n", &Config::native_math()).unwrap_err();
    assert!(
        matches!(&err, CrateError::InvalidMarkdown(m) if m.contains("line 3")),
        "{err}"
    );
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

// ------------------------------------------------------- general markdown

#[test]
fn common_markdown_maps_onto_word_structures() {
    let md = "\
---
title: ignored front matter
---

# One

## Two

Plain **bold** *italic* ~~gone~~ `code` and a [link](https://example.com/a?b=1&c=2).

> quoted text

1. first
2. second
   - nested bullet
3. third

- [x] done
- [ ] todo

| Name | Qty |
|:-----|----:|
| a    | 1   |
| b    | 2   |

---

```rust
fn main() {}
```
";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);

    assert!(body.contains(r#"<w:pStyle w:val="Heading1"/>"#) && body.contains(r#"<w:pStyle w:val="Heading2"/>"#));
    assert!(!body.contains("ignored front matter"));
    assert!(body.contains("<w:b/>") && body.contains("<w:i/>"));
    assert!(body.contains("<w:strike"));
    assert!(body.contains("<w:hyperlink"));
    assert!(body.contains("<w:numPr>"));
    assert!(body.contains("<w:tbl>") && (body.matches("<w:tr>").count() + body.matches("<w:tr ").count()) == 3);
    assert!(body.contains("☑") && body.contains("☐"));
    assert!(body.contains("fn main() {}"));
    assert!(body.contains(r#"w:val="CodeBlock""#));

    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(rels.contains("https://example.com/a?b=1&amp;c=2"), "{rels}");
    let numbering = zip_text(&docx, "word/numbering.xml");
    assert!(numbering.contains("decimal") && numbering.contains("bullet"));
}

#[test]
fn local_images_are_embedded_relative_to_base_dir() {
    let fx = Fixture::new();
    let cfg = Config {
        base_dir: Some(fx.dir.path().to_path_buf()),
        ..Config::default()
    };
    let docx = markdown_to_docx("![logo](fixture.png) and ![remote](https://example.com/x.png)", &cfg).unwrap();
    assert_eq!(media_count(&docx), 1);
    assert!(zip_text(&docx, "word/document.xml").contains("[remote]"));

    match markdown_to_docx("![x](missing.png)", &cfg) {
        Err(CrateError::Io { path, .. }) => assert!(path.ends_with("missing.png")),
        other => panic!("expected Io error, got {other:?}"),
    }
}

#[test]
fn output_is_a_complete_docx_package() {
    let docx = markdown_to_docx("hello", &Config::default()).unwrap();
    let names = zip_entries(&docx);
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
    ] {
        assert!(names.iter().any(|n| n == required), "missing {required} in {names:?}");
    }
    assert_eq!(&docx[..2], b"PK");
}
