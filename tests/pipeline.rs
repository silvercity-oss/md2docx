//! End-to-end tests for the Markdown -> .docx pipeline that need no external
//! tools, so they run on every platform. See `pipeline_mock_tools.rs` for the
//! tests that drive renderers through stand-in executables.

mod common;

use common::*;
use silvercity_oss::__internals::preprocess;
use silvercity_oss::{markdown_to_docx, CommandSpec, Config, CrateError, MathImageEngine};

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
    assert!(!body.contains("SILVERCITYOMML"));
    assert!(body.contains("Inline ") && body.contains(" here.") && body.contains("Tail."));
    // The two forms: exactly one display paragraph, two equations in total.
    assert_eq!(body.matches("<m:oMathPara>").count(), 1);
    assert_eq!(body.matches("<m:oMath>").count(), 2);
    assert_eq!(
        body.matches("xmlns:m=").count(),
        1,
        "math namespace is declared once, on the root"
    );
    let root = &body[..body.find("<w:body>").unwrap()];
    assert!(
        root.contains(r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#),
        "namespace must be on <w:document> with the real OMML URI: {root}"
    );
}

#[test]
fn omml_equation_inside_table_list_and_heading_survives() {
    let cfg = Config::native_math();
    let md = "# Title $E=mc^2$\n\n- item $a+b$\n\n| h | v |\n|---|---|\n| $$\\pi$$ | $c^2$ |\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(!body.contains("SILVERCITYOMML"));
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

#[test]
fn missing_cli_is_reported_by_name() {
    let mut cfg = Config::trusted();
    cfg.graphs.mermaid.program = "definitely-not-installed-mmdc".into();
    match markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg) {
        Err(CrateError::MissingExternalCli { program }) => assert_eq!(program, "definitely-not-installed-mmdc"),
        other => panic!("expected MissingExternalCli, got {other:?}"),
    }
    let mut cfg = Config::trusted();
    cfg.math.engine = MathImageEngine::LatexDvipng {
        latex: "no-such-latex".into(),
        dvipng: "no-such-dvipng".into(),
    };
    assert!(matches!(
        markdown_to_docx("$x$", &cfg),
        Err(CrateError::MissingExternalCli { program }) if program == "no-such-latex"
    ));
}

#[test]
fn invalid_input_is_rejected_not_panicked_on() {
    assert!(matches!(
        markdown_to_docx("```mermaid\n   \n```\n", &Config::trusted()),
        Err(CrateError::InvalidMarkdown(m)) if m.contains("line 1")
    ));
    assert!(matches!(
        markdown_to_docx("a\0b", &Config::trusted()),
        Err(CrateError::InvalidMarkdown(_))
    ));
    let err = markdown_to_docx("ok\n\n$$\\left( x$$\n", &Config::native_math()).unwrap_err();
    assert!(
        matches!(&err, CrateError::InvalidMarkdown(m) if m.contains("line 3")),
        "{err}"
    );
}

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
    let docx = markdown_to_docx(md, &Config::trusted()).unwrap();
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
    let cfg = Config::trusted().with_base_dir(fx.dir.path());
    let docx = markdown_to_docx("![logo](fixture.png) and ![remote](https://example.com/x.png)", &cfg).unwrap();
    assert_eq!(media_count(&docx), 1);
    assert!(zip_text(&docx, "word/document.xml").contains("[remote]"));

    // A missing file costs one picture, not the document, and the finding names no host path.
    let r = silvercity_oss::convert("![x](missing.png)", &cfg).unwrap();
    assert!(r.diagnostics.iter().any(|d| d.code.as_str() == "image.unreadable"));
    assert!(r
        .diagnostics
        .iter()
        .all(|d| !d.message.contains(&*fx.dir.path().to_string_lossy())));
    assert!(zip_text(&r.docx, "word/document.xml").contains("[x]"));
}

#[test]
fn output_is_a_complete_docx_package() {
    let docx = markdown_to_docx("hello", &Config::trusted()).unwrap();
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
