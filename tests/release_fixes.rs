//! Regression tests for the second review: heading anchors and internal links, footnote
//! numbers, document properties, page numbers, the code-block frame, the failed-diagram
//! policy, error classification and the single-paragraph budget.

mod common;

use common::*;
use silvercity_oss::{
    markdown_to_docx, CodeBlockLayout, Config, CrateError, DiagramFailure, DocumentOptions, FenceError, Highlighting,
    Theme,
};

fn docx(md: &str, cfg: &Config) -> Vec<u8> {
    markdown_to_docx(md, cfg).unwrap_or_else(|e| panic!("{e}"))
}

// ---- internal links

fn bookmark_names(doc: &str) -> Vec<String> {
    doc.split("w:name=\"")
        .skip(1)
        .map(|c| c.split('"').next().unwrap().to_string())
        .collect()
}

fn anchors_used(doc: &str) -> Vec<String> {
    doc.split("w:anchor=\"")
        .skip(1)
        .map(|c| c.split('"').next().unwrap().to_string())
        .collect()
}

#[test]
fn links_to_headings_become_internal_hyperlinks() {
    let md = "# Table of contents\n\n- [Intro](#introduction)\n- [Usage](#usage-notes)\n- [Again](#usage-notes-1)\n- [Custom](#my-id)\n- [Umlaut](#%C3%BCberblick)\n- [Shouty](#INTRODUCTION)\n- [Missing](#nowhere)\n\n## Introduction\n\ntext\n\n## Usage notes\n\n## Usage notes\n\n## Anything {#my-id}\n\n## Überblick\n";
    let d = docx(md, &Config::native_math());
    let doc = zip_text(&d, "word/document.xml");
    assert_well_formed(&doc);
    let names = bookmark_names(&doc);
    // One bookmark per heading, numbered in document order.
    assert_eq!(names, ["md_h0", "md_h1", "md_h2", "md_h3", "md_h4", "md_h5"], "{doc}");
    // Every internal link points at a bookmark that exists.
    let used = anchors_used(&doc);
    assert_eq!(used, ["md_h1", "md_h2", "md_h3", "md_h4", "md_h5", "md_h1"], "{used:?}");
    for a in &used {
        assert!(names.contains(a), "{a}");
    }
    // Bookmark ids are unique and each start has an end.
    let starts = doc.matches("<w:bookmarkStart").count();
    assert_eq!(starts, 6);
    assert_eq!(doc.matches("<w:bookmarkEnd").count(), 6);
    // The link to a heading that does not exist stays plain text and creates no relationship.
    assert!(visible_text(&doc).contains("Missing"));
    assert!(!zip_text(&d, "word/_rels/document.xml.rels").contains("nowhere"));
}

#[test]
fn internal_links_work_in_untrusted_mode_and_never_reach_outside() {
    let md = "[a](#top) [b](javascript:alert(1)) [c](#)\n\n# Top\n";
    let d = docx(md, &Config::untrusted());
    let doc = zip_text(&d, "word/document.xml");
    assert_eq!(anchors_used(&doc), ["md_h0"]);
    assert!(!zip_text(&d, "word/_rels/document.xml.rels").contains("javascript"));
}

#[test]
fn heading_links_in_footnotes_point_at_the_body_bookmark() {
    let md = "# H\n\nNote[^1]\n\n[^1]: back to [the top](#h)\n";
    let d = docx(md, &Config::native_math());
    let notes = zip_text(&d, "word/footnotes.xml");
    assert!(visible_text(&notes).contains("the top"));
    let anchor = notes
        .split("w:anchor=\"")
        .nth(1)
        .expect("an internal link")
        .split('"')
        .next()
        .unwrap();
    let body = zip_text(&d, "word/document.xml");
    assert!(
        body.contains(&format!("w:name=\"{anchor}\"")),
        "no bookmark {anchor} in the body"
    );
    assert!(
        !notes.contains("r:id"),
        "an internal link needs no relationship: {notes}"
    );
}

// ---- footnote numbers

#[test]
fn every_footnote_starts_with_its_number() {
    let md = "A[^1] B[^2].\n\n[^1]: first\n\n[^2]: second\n\n    more\n";
    let d = docx(md, &Config::native_math());
    let notes = zip_text(&d, "word/footnotes.xml");
    assert_well_formed(&notes);
    assert_eq!(notes.matches("<w:footnote ").count(), 2);
    assert_eq!(
        notes.matches("<w:footnoteRef/>").count(),
        2,
        "one per note, not per paragraph: {notes}"
    );
    for note in notes.split("<w:footnote ").skip(1) {
        let first = note.split("<w:t ").nth(1).unwrap_or("");
        let before_text = &note[..note.find("<w:t ").unwrap()];
        assert!(
            before_text.contains("<w:footnoteRef/>"),
            "the number comes first: {note}"
        );
        assert!(
            first.starts_with("xml:space=\"preserve\"> </w:t>"),
            "followed by a space"
        );
    }
    let styles = zip_text(&d, "word/styles.xml");
    assert!(
        styles.contains("FootnoteReference") && styles.contains("superscript"),
        "{styles}"
    );
}

// ---- document properties and page numbers

#[test]
fn properties_come_from_the_options_or_the_front_matter() {
    let core = |md: &str, cfg: &Config| zip_text(&docx(md, cfg), "docProps/core.xml");
    let c = core(
        "---\ntitle: \"Annual <Report> & more\"\nauthor: Ada\n---\n# x\n",
        &Config::native_math(),
    );
    assert!(
        c.contains("<dc:title>Annual &lt;Report&gt; &amp; more</dc:title>"),
        "{c}"
    );
    assert!(c.contains("<dc:creator>Ada</dc:creator>") && c.contains("<cp:lastModifiedBy>Ada</cp:lastModifiedBy>"));
    // The options win over the front matter.
    let mut opts = DocumentOptions::default();
    opts.title = Some("Mine".into());
    opts.author = Some("Grace".into());
    let c = core(
        "---\ntitle: Theirs\nauthor: Them\n---\n# x\n",
        &Config::native_math().with_document(opts),
    );
    assert!(
        c.contains("<dc:title>Mine</dc:title>") && c.contains("<dc:creator>Grace</dc:creator>"),
        "{c}"
    );
    // TOML front matter, and nothing at all.
    let c = core("+++\ntitle = 'T'\n+++\nx\n", &Config::native_math());
    assert!(c.contains("<dc:title>T</dc:title>"));
    let c = core("x\n", &Config::native_math());
    assert!(!c.contains("<dc:title>") && c.contains("<dc:creator>unknown</dc:creator>"));
    // Dates are the real time.
    assert!(!c.contains("1970-01-01"), "{c}");
    let year: u32 = c.split("W3CDTF\">").nth(1).unwrap()[..4].parse().unwrap();
    assert!((2024..2200).contains(&year), "{year}");
}

#[test]
fn bad_properties_are_refused() {
    for (title, author) in [
        (Some("x".repeat(201)), None),
        (None, Some("a\u{7}b".to_string())),
        (Some("\u{fffe}".into()), None),
    ] {
        let mut opts = DocumentOptions::default();
        opts.title = title;
        opts.author = author;
        assert!(matches!(
            markdown_to_docx("x", &Config::trusted().with_document(opts)),
            Err(CrateError::InvalidConfig(_))
        ));
    }
    // Hostile front matter values are cut and cleaned, never trusted.
    let md = format!(
        "---\ntitle: {}\nauthor: a\u{7}b</dc:creator><x/>\n---\nx\n",
        "t".repeat(500)
    );
    let c = zip_text(&docx(&md, &Config::untrusted()), "docProps/core.xml");
    assert_well_formed(&c);
    assert!(!c.contains("<x/>"), "{c}");
    assert!(c.contains(&format!("<dc:title>{}</dc:title>", "t".repeat(200))));
}

#[test]
fn page_numbers_are_a_footer_field() {
    let plain = docx("x", &Config::native_math());
    assert!(!zip_entries(&plain).iter().any(|n| n.contains("footer")));
    let mut opts = DocumentOptions::default();
    opts.page_numbers = true;
    let d = docx("x", &Config::native_math().with_document(opts));
    let footer = zip_entries(&d)
        .into_iter()
        .find(|n| n.starts_with("word/footer"))
        .expect("a footer part");
    let xml = zip_text(&d, &footer);
    assert_well_formed(&xml);
    assert!(xml.contains("PAGE"), "{xml}");
}

// ---- code block frame

#[test]
fn code_blocks_are_tight_inside_and_separate_from_each_other() {
    let md = "```\na\nb\n```\n\n```\nc\n```\n\nafter\n";
    let doc = body_with(md, &Config::native_math());
    let paras: Vec<&str> = doc.split("<w:p ").skip(1).collect();
    // a, b, spacer, c, spacer, after: the style has no spacing between lines, and each block
    // is followed by a thin unstyled paragraph so two blocks never merge into one box.
    let kinds: Vec<bool> = paras.iter().map(|p| p.contains("w:val=\"CodeBlock\"")).collect();
    assert_eq!(kinds, [true, true, false, true, false, false], "{doc}");
    assert!(
        paras[2].contains("w:sz w:val=\"8\"") && !paras[2].contains("<w:t"),
        "{}",
        paras[2]
    );
    let styles = zip_text(&docx(md, &Config::native_math()), "word/styles.xml");
    assert!(styles.contains("w:before=\"0\"") && styles.contains("w:after=\"0\""));
    // No frame unless the theme asks for one; the light theme does, since its page is white.
    assert!(!doc.contains("<w:pBdr>"));
    let light = body_with(
        md,
        &Config::native_math().with_highlighting(Highlighting::new(Theme::vscode_light())),
    );
    for side in ["w:top", "w:left", "w:bottom", "w:right"] {
        assert!(light.contains(&format!("<{side} w:val=\"single\"")), "{side}");
    }
    assert!(light.contains("w:color=\"D0D0D0\""));
}

// ---- failed diagrams

fn broken_mermaid(cfg: Config) -> Config {
    let mut cfg = cfg;
    cfg.graphs.mermaid.program = "/nonexistent/merman-cli".into();
    cfg
}

#[test]
fn a_diagram_that_cannot_be_rendered_can_stay_code() {
    let md = "before\n\n```mermaid\ngraph TD; A-->B\n```\n\nafter\n";
    let strict = broken_mermaid(Config::trusted());
    assert!(matches!(
        markdown_to_docx(md, &strict),
        Err(CrateError::MissingExternalCli { .. })
    ));
    let mut soft = strict.clone();
    soft.graphs.on_failure = DiagramFailure::KeepAsCode;
    let doc = body_with(md, &soft);
    let text = visible_text(&doc);
    assert!(
        text.contains("before") && text.contains("graph TD; A-->B") && text.contains("after"),
        "{text}"
    );
    assert!(doc.contains("CodeBlock") && !doc.contains("<w:drawing>"));

    // A registered renderer that fails is treated the same way.
    let failing = |on: DiagramFailure| {
        let mut c = Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
            Err("bad data".into())
        });
        c.graphs.on_failure = on;
        c
    };
    assert!(markdown_to_docx("```chart\nx\n```\n", &failing(DiagramFailure::Error)).is_err());
    let doc = body_with("```chart\nx\n```\n", &failing(DiagramFailure::KeepAsCode));
    assert!(visible_text(&doc).contains('x'));

    // Limits are never softened.
    let mut soft_limit = soft.clone();
    soft_limit.limits.max_graph_source_bytes = 4;
    assert!(matches!(
        markdown_to_docx(md, &soft_limit),
        Err(CrateError::LimitExceeded(_))
    ));
}

#[test]
fn the_cli_can_keep_failed_diagrams() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("d.md");
    std::fs::write(&input, "```mermaid\ngraph TD; A-->B\n```\n").unwrap();
    let run = |extra: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .args(["--mermaid-command", "/nonexistent/tool {input} {output}"])
            .args(extra)
            .output()
            .unwrap()
    };
    assert_eq!(run(&[]).status.code(), Some(1));
    let ok = run(&["--keep-failed-diagrams"]);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
}

#[test]
fn errors_say_whose_fault_they_are() {
    let input = |md: &str, cfg: &Config| markdown_to_docx(md, cfg).unwrap_err().is_input_error();
    assert!(input("$\\left( x$", &Config::native_math()));
    assert!(input(&"x".repeat(3 << 20), &Config::untrusted()));
    assert!(input("![x](/etc/passwd)", &Config::native_math()));
    let mut bad = Config::trusted();
    bad.compression_level = 99;
    assert!(!input("x", &bad));
    assert!(!input("```mermaid\ng\n```\n", &broken_mermaid(Config::trusted())));
}

// ---- single paragraph budget

#[test]
fn single_paragraph_blank_lines_count_against_the_budget() {
    let mut opts = DocumentOptions::default();
    opts.code_blocks = CodeBlockLayout::SingleParagraph;
    let cfg = Config::untrusted().with_document(opts);
    let md = format!("```\n{}```\n", "\n".repeat(70_000));
    assert!(matches!(
        markdown_to_docx(&md, &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("max_elements")
    ));
}
