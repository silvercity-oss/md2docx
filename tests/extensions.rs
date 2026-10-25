//! Extension points: custom fence renderers, custom HTML tag styles, document
//! colours, and the output size limit.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::*;
use silvercity_oss::{markdown_to_docx, BlockStyle, Config, CrateError, DocumentOptions, FenceError, TagStyle};

fn png() -> Vec<u8> {
    png_bytes(120, 60)
}

fn chart_cfg() -> Config {
    Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) })
}

// ---- fence renderers

#[test]
fn a_registered_renderer_turns_a_fence_into_a_picture() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = Arc::clone(&seen);
    let cfg = Config::trusted().with_fence_renderer("chart", move |src: &str| -> Result<Vec<u8>, FenceError> {
        log.lock().unwrap().push(src.to_string());
        Ok(png())
    });
    let docx = markdown_to_docx("before\n\n```chart\n1 2 3\n```\n\nafter\n", &cfg).unwrap();
    assert_eq!(media_count(&docx), 1);
    let b = zip_text(&docx, "word/document.xml");
    assert_eq!(b.matches("<w:drawing>").count(), 1);
    assert_eq!(seen.lock().unwrap().as_slice(), ["1 2 3\n"]);
    let text = visible_text(&b);
    assert!(text.contains("before") && text.contains("after") && !text.contains("1 2 3"));
}

#[test]
fn language_matching_ignores_case_and_attributes() {
    let n = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&n);
    let cfg = Config::trusted().with_fence_renderer("Chart", move |_: &str| -> Result<Vec<u8>, FenceError> {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(png())
    });
    let md = "```CHART title=\"x\"\na\n```\n\n```chart\nb\n```\n\n```charts\nc\n```\n\n```\nchart\n```\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    assert_eq!(
        n.load(Ordering::SeqCst),
        2,
        "`charts` and an untagged fence are not `chart`"
    );
    assert_eq!(zip_text(&docx, "word/document.xml").matches("<w:drawing>").count(), 2);
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(
        text.contains('c') && text.contains("chart"),
        "other fences stay code: {text}"
    );
}

#[test]
fn identical_blocks_are_rendered_once() {
    let n = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&n);
    let cfg = Config::trusted().with_fence_renderer("chart", move |_: &str| -> Result<Vec<u8>, FenceError> {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(png())
    });
    let md = "```chart\nsame\n```\n\n```chart\nsame\n```\n\n```chart\nother\n```\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    assert_eq!(n.load(Ordering::SeqCst), 2);
    assert_eq!(zip_text(&docx, "word/document.xml").matches("<w:drawing>").count(), 3);
}

#[test]
fn renderers_work_in_the_untrusted_preset_and_can_replace_mermaid() {
    // Untrusted: diagrams off and nothing is spawned, but the embedder's own renderer runs.
    let cfg =
        Config::untrusted().with_fence_renderer("mermaid", |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) });
    let docx = markdown_to_docx(
        "```mermaid\ngraph TD; A-->B\n```\n\n```dolphin\nstill code\n```\n",
        &cfg,
    )
    .unwrap();
    assert_eq!(media_count(&docx), 1);
    assert!(visible_text(&zip_text(&docx, "word/document.xml")).contains("still code"));

    // With the command line pointed at nothing, the renderer still wins.
    let mut cfg =
        Config::trusted().with_fence_renderer("mermaid", |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) });
    cfg.graphs.mermaid.program = "/nonexistent/never-run".into();
    assert_eq!(
        media_count(&markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg).unwrap()),
        1
    );

    // The hardened preset accepts it too.
    let cfg = Config::untrusted_with_diagrams()
        .with_fence_renderer("mermaid", |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) });
    assert_eq!(
        media_count(&markdown_to_docx("```mermaid\ngraph TD; A-->B\n```\n", &cfg).unwrap()),
        1
    );
}

#[test]
fn a_failing_renderer_aborts_with_its_message() {
    let cfg = Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        Err("no data in block".into())
    });
    match markdown_to_docx("```chart\nx\n```\n", &cfg) {
        Err(CrateError::ExternalToolFailed { program, detail }) => {
            assert!(program.contains("chart"), "{program}");
            assert!(detail.contains("no data in block"), "{detail}");
        }
        other => panic!("expected ExternalToolFailed, got {:?}", other.map(|d| d.len())),
    }
}

#[test]
fn a_panicking_renderer_is_an_error_not_an_unwind() {
    let cfg = Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        panic!("renderer bug")
    });
    assert!(matches!(
        markdown_to_docx("```chart\nx\n```\n", &cfg),
        Err(CrateError::ExternalToolFailed { detail, .. }) if detail.contains("panicked")
    ));
}

#[test]
fn renderer_output_is_checked_like_any_other_image() {
    let not_png = Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        Ok(b"<svg/>".to_vec())
    });
    assert!(matches!(
        markdown_to_docx("```chart\nx\n```\n", &not_png),
        Err(CrateError::ExternalToolFailed { .. })
    ));

    let mut big = chart_cfg();
    big.limits.max_image_bytes = 100;
    assert!(matches!(
        markdown_to_docx("```chart\nx\n```\n", &big),
        Err(CrateError::LimitExceeded(_))
    ));

    // A PNG whose header claims an absurd size is not embedded as a huge picture.
    let bomb = Config::trusted().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        Ok(png_claiming(60_000, 60_000))
    });
    let docx = markdown_to_docx("```chart\nx\n```\n", &bomb);
    assert!(
        !matches!(&docx, Ok(d) if media_count(d) > 0),
        "a decompression-bomb header must never become a picture"
    );
}

#[test]
fn renderer_blocks_obey_the_diagram_limits() {
    let mut cfg = chart_cfg();
    cfg.limits.max_diagrams = 2;
    let three = "```chart\na\n```\n\n```chart\nb\n```\n\n```chart\nc\n```\n";
    assert!(matches!(
        markdown_to_docx(three, &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("diagrams")
    ));
    let mut cfg = chart_cfg();
    cfg.limits.max_graph_source_bytes = 4;
    assert!(matches!(
        markdown_to_docx("```chart\n0123456789\n```\n", &cfg),
        Err(CrateError::LimitExceeded(_))
    ));
    assert!(matches!(
        markdown_to_docx("```chart\n\n```\n", &chart_cfg()),
        Err(CrateError::InvalidMarkdown(_))
    ));
}

#[test]
fn fence_renderer_names_are_validated() {
    for bad in ["", "has space", "a/b", "<x>", &"x".repeat(33)] {
        let cfg = chart_cfg().with_fence_renderer(bad, |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) });
        assert!(
            matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))),
            "`{bad}` should be rejected"
        );
    }
    // Upper case is folded to lower case at registration.
    let cfg = chart_cfg().with_fence_renderer("C#", |_: &str| -> Result<Vec<u8>, FenceError> { Ok(png()) });
    assert!(markdown_to_docx("x", &cfg).is_ok());
    assert!(
        format!("{:?}", chart_cfg()).contains("chart"),
        "Debug lists the languages"
    );
}

// ---- tag styles

#[test]
fn inline_tag_styles_apply_to_their_content_only() {
    let cfg = Config::trusted()
        .with_html_tag("mark", TagStyle::new().highlight("FFFF00"))
        .with_html_tag("warn", TagStyle::new().bold().color("C00000"))
        .with_html_tag("call-out", TagStyle::new().italic().strike().underline().monospace());
    let b = body_with(
        "plain <mark>lit</mark> <warn>red</warn> <call-out>odd</call-out> end\n",
        &cfg,
    );
    assert!(b.contains("w:fill=\"FFFF00\""), "{b}");
    assert!(
        b.contains("w:color w:val=\"C00000\"") || b.contains("w:val=\"C00000\""),
        "{b}"
    );
    assert!(
        b.contains("<w:b/>") && b.contains("<w:i/>") && b.contains("<w:strike/>"),
        "{b}"
    );
    assert!(b.contains("InlineCode"), "{b}");
    let text = visible_text(&b);
    assert!(text.contains("plain lit red odd end"), "{text}");
    // `plain` and `end` are outside every tag: one run each carries no formatting.
    assert!(b.contains("<w:t xml:space=\"preserve\">plain </w:t>"), "{b}");
    assert_eq!(
        b.matches("w:fill=\"FFFF00\"").count(),
        1,
        "only `lit` is highlighted: {b}"
    );
}

#[test]
fn unregistered_tags_show_their_text_and_nothing_else() {
    let b = body("a <mark>lit</mark> <callout>note</callout> z\n");
    assert!(visible_text(&b).contains("a lit note z"));
    assert!(!b.contains("FFFF00"));
}

#[test]
fn block_tags_make_laid_out_paragraphs() {
    let cfg = Config::untrusted().with_html_tag(
        "callout",
        TagStyle::new().block(
            BlockStyle::new()
                .fill("FFF4CE")
                .bar_color("E0A800")
                .indent_twips(360)
                .label("Note: "),
        ),
    );
    let b = body_with("before\n\n<callout>mind the gap</callout>\n\nafter\n", &cfg);
    let text = visible_text(&b);
    assert!(text.contains("beforeNote: mind the gapafter"), "{text}");
    assert_eq!(
        b.matches("w:fill=\"FFF4CE\"").count(),
        1,
        "only the callout paragraph: {b}"
    );
    assert!(b.contains("w:color=\"E0A800\""), "bar colour: {b}");
    assert!(b.contains("w:left=\"360\""), "indent: {b}");
    assert!(b.contains("Note: "), "label");

    // Markdown inside the block is laid out the same way, paragraph by paragraph.
    let md = "<callout>\n\nfirst **bold** paragraph\n\nsecond paragraph\n\n</callout>\n\nafter\n";
    let b = body_with(md, &cfg);
    assert!(b.matches("w:fill=\"FFF4CE\"").count() >= 2, "{b}");
    let text = visible_text(&b);
    assert!(
        text.contains("first bold paragraph") && text.contains("second paragraph"),
        "{text}"
    );
    // The paragraph after the closing tag is plain again.
    let after = b.rsplit("<w:p ").next().unwrap();
    assert!(after.contains("after") && !after.contains("FFF4CE"), "{after}");
}

#[test]
fn custom_styles_can_override_builtin_tags() {
    let cfg = Config::trusted().with_html_tag("b", TagStyle::new().italic());
    let b = body_with("<b>word</b>\n", &cfg);
    assert!(b.contains("<w:i/>") && !b.contains("<w:b/>"), "{b}");
}

#[test]
fn a_document_cannot_choose_its_own_styling() {
    let cfg = Config::trusted().with_html_tag("mark", TagStyle::new().highlight("FFFF00"));
    let b = body_with(
        "<mark style=\"background:red;color:blue\" class=\"x\" fill=\"00FF00\">t</mark>\n",
        &cfg,
    );
    assert!(b.contains("FFFF00") && !b.contains("00FF00") && !b.to_lowercase().contains("w:fill=\"red\""));
    assert!(!b.contains("0000FF"));
}

#[test]
fn runaway_custom_tags_are_bounded() {
    let cfg = Config::trusted()
        .with_html_tag("x", TagStyle::new().bold())
        .with_html_tag("blk", TagStyle::new().block(BlockStyle::new().fill("EEEEEE")));
    for md in [
        "<x>a".repeat(50_000),
        "<blk>a".repeat(20_000),
        "</x></blk>".repeat(20_000),
    ] {
        let b = body_with(&md, &cfg);
        assert_well_formed(&b);
    }
    // An unclosed inline tag does not escape the document, and a stray close is harmless.
    let b = body_with("</x>text <x>bold", &cfg);
    assert!(visible_text(&b).contains("text bold"));
}

#[test]
fn tag_styles_are_validated_before_any_work() {
    let bad_names = [
        "Mark",
        "1a",
        "a b",
        "script",
        "style",
        "iframe",
        "",
        "a_b",
        "<x>",
        &"a".repeat(33),
    ];
    for name in bad_names {
        let cfg = Config::trusted().with_html_tag(name, TagStyle::new().bold());
        assert!(
            matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))),
            "`{name}` should be rejected"
        );
    }
    let bad_styles = [
        TagStyle::new().color("red"),
        TagStyle::new().color("FFF"),
        TagStyle::new().highlight("GG0000"),
        TagStyle::new().color("\"/><w:x/>"),
        TagStyle::new().block(BlockStyle::new().fill("12345")),
        TagStyle::new().block(BlockStyle::new().bar_color("zzzzzz")),
        TagStyle::new().block(BlockStyle::new().indent_twips(5000)),
        TagStyle::new().block(BlockStyle::new().label("a\u{7}b")),
        TagStyle::new().block(BlockStyle::new().label("x".repeat(65))),
    ];
    for style in bad_styles {
        let cfg = Config::trusted().with_html_tag("ok", style.clone());
        assert!(
            matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))),
            "{style:?} should be rejected"
        );
    }
    let mut many = Config::trusted();
    for i in 0..65 {
        many = many.with_html_tag(format!("t{i}"), TagStyle::new());
    }
    assert!(matches!(
        markdown_to_docx("x", &many),
        Err(CrateError::InvalidConfig(_))
    ));
}

#[test]
fn labels_are_escaped() {
    let cfg = Config::trusted().with_html_tag("n", TagStyle::new().block(BlockStyle::new().label("<&> ")));
    let b = body_with("<n>x</n>\n", &cfg);
    assert_well_formed(&b);
    assert!(visible_text(&b).contains("<&> x"));
}

// ---- document colours

#[test]
fn document_colours_are_configurable_and_validated() {
    let mut doc = DocumentOptions::default();
    doc.code_background = "E8F0FE".into();
    doc.link_color = "AA0000".into();
    doc.quote_bar_color = "00AA00".into();
    let cfg = Config::trusted().with_document(doc.clone());
    let docx = markdown_to_docx("```\ncode\n```\n\n`in`\n\n> quote\n\n[l](https://e.com/)\n", &cfg).unwrap();
    let b = zip_text(&docx, "word/document.xml");
    assert_eq!(b.matches("w:fill=\"E8F0FE\"").count(), 2, "block and inline code: {b}");
    assert!(!b.contains("F2F2F2"));
    assert!(b.contains("w:color=\"00AA00\""), "quote bar");
    assert!(zip_text(&docx, "word/styles.xml").contains("AA0000"), "hyperlink style");

    for field in 0..3 {
        let mut bad = doc.clone();
        let slot = match field {
            0 => &mut bad.code_background,
            1 => &mut bad.link_color,
            _ => &mut bad.quote_bar_color,
        };
        *slot = "not-hex".into();
        let cfg = Config::trusted().with_document(bad);
        assert!(matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))));
    }
}

// ---- output limit
