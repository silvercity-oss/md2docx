//! Opt-in Markdown/HTML conveniences and layout details: autolinks, super/subscript, HTML
//! anchors, content-sized table columns, mailto filtering.

mod common;

use common::*;
use md2docx::{markdown_to_docx, Config};

fn body_with(md: &str, cfg: &Config) -> String {
    let b = zip_text(&markdown_to_docx(md, cfg).unwrap(), "word/document.xml");
    assert_well_formed(&b);
    b
}

fn rels_with(md: &str, cfg: &Config) -> String {
    zip_text(&markdown_to_docx(md, cfg).unwrap(), "word/_rels/document.xml.rels")
}

fn grid(xml: &str) -> Vec<usize> {
    let start = xml.find("<w:tblGrid>").expect("a table grid");
    let end = xml[start..].find("</w:tblGrid>").unwrap() + start;
    xml[start..end]
        .split("w:w=\"")
        .skip(1)
        .map(|s| s[..s.find('"').unwrap()].parse().unwrap())
        .collect()
}

#[test]
fn table_columns_follow_their_content() {
    let md = "| id | description | ok |\n|---|---|---|\n| 1 | A long explanation of what this row is about, in prose | y |\n";
    let g = grid(&body_with(md, &Config::default()));
    assert_eq!(g.len(), 3);
    assert!(g[1] > 3 * g[0] && g[1] > 3 * g[2], "{g:?}");
    let total: usize = g.iter().sum();
    assert!(
        (total as i64 - 9026).abs() < 400,
        "the table keeps the text width: {g:?}"
    );
}

#[test]
fn bare_addresses_become_links_only_when_asked_and_only_to_allowed_schemes() {
    let md = "Visit https://example.com/docs. Also www.example.org, ftp://example.net/f and (https://x.example/a_(b)).";
    let off = rels_with(md, &Config::default());
    assert!(!off.contains("example.com"), "off by default: {off}");
    let cfg = Config::default().with_autolink_bare_urls(true);
    let rels = rels_with(md, &cfg);
    assert!(rels.contains("Target=\"https://example.com/docs\""), "{rels}");
    assert!(rels.contains("Target=\"https://www.example.org\""), "{rels}");
    assert!(rels.contains("https://x.example/a_(b)"), "{rels}");
    assert!(!rels.contains("ftp://"), "ftp is not an allowed scheme");
    let body = body_with(md, &cfg);
    assert!(visible_text(&body).contains("ftp://example.net/f"), "the text stays");
    // Not inside an explicit link, or in code.
    let inside = rels_with("[https://a.example/x](https://b.example/y) `https://c.example`", &cfg);
    assert!(
        inside.contains("b.example") && !inside.contains("a.example") && !inside.contains("c.example"),
        "{inside}"
    );
}

#[test]
fn superscript_and_subscript_are_opt_in() {
    let md = "H ~2~ O and x ^2^ and ~~gone~~";
    let off = body_with(md, &Config::default());
    assert!(!off.contains("w:vertAlign"), "{off}");
    assert!(off.contains("<w:strike"), "single tildes stay strikethrough by default");
    let on = body_with(md, &Config::default().with_superscript_subscript(true));
    assert!(
        on.contains("w:val=\"subscript\"") && on.contains("w:val=\"superscript\""),
        "{on}"
    );
    assert!(on.contains("<w:strike"), "double tildes still strike");
}

#[test]
fn html_anchors_are_link_targets() {
    let md = "<a name=\"intro\"></a>\n\nJump to [the intro](#intro) or <a href=\"#intro\">here</a>.\n\n<div id=\"box\">Box</div>\n\n[to box](#box) and [nowhere](#nope)\n";
    let b = body_with(md, &Config::default());
    assert!(b.contains("<w:bookmarkStart w:id=\"1048576\" w:name=\"md_a0\""), "{b}");
    assert!(b.matches("w:anchor=\"md_a0\"").count() == 2, "{b}");
    assert!(
        b.contains("w:anchor=\"md_a1\""),
        "the id of any element is a target: {b}"
    );
    assert!(!b.contains("md_a2"));
    let diags = md2docx::convert(md, &Config::default()).unwrap().diagnostics;
    assert!(
        diags.iter().any(|d| d.code == md2docx::Code::LinkAnchorMissing),
        "{diags:?}"
    );
}

#[test]
fn headings_keep_their_names_when_an_html_anchor_reuses_them() {
    let b = body_with("# Same\n\n<a name=\"same\"></a>\n\n[go](#same)\n", &Config::default());
    assert!(b.contains("w:anchor=\"md_h0\""), "the heading wins the name: {b}");
}

#[test]
fn mailto_links_lose_everything_but_the_prefill_fields() {
    let cfg = Config::default().with_html_links(true);
    let rels = rels_with("[m](mailto:a@b.example?subject=Hi&attach=/etc/passwd)", &cfg);
    assert!(rels.contains("mailto:a@b.example?subject=Hi\""), "{rels}");
    assert!(!rels.contains("attach"));
}

// ---- table of contents and heading numbers

use md2docx::{DocumentOptions, TableOfContents};

fn with_doc(f: impl FnOnce(&mut DocumentOptions)) -> Config {
    let mut d = DocumentOptions::default();
    f(&mut d);
    Config::default().with_document(d)
}

const OUTLINE: &str = "# Alpha\n\n## Beta\n\n### Gamma\n\n#### Delta\n\n# Epsilon <b>bold</b>\n\ntext\n";

#[test]
fn a_table_of_contents_lists_headings_as_links_to_their_bookmarks() {
    let cfg = with_doc(|d| d.table_of_contents = Some(TableOfContents::new()));
    let b = body_with(OUTLINE, &cfg);
    let text = visible_text(&b);
    assert!(text.starts_with("Contents"), "{text}");
    for entry in ["Alpha", "Beta", "Gamma", "Epsilon"] {
        assert!(
            text.matches(entry).count() >= 2,
            "{entry} appears in the contents and as a heading"
        );
    }
    // Level 4 is deeper than the default max_level 3: heading only.
    assert_eq!(text.matches("Delta").count(), 1, "{text}");
    for n in 0..5 {
        assert!(b.contains(&format!("w:anchor=\"md_h{n}\"")) == (n != 3), "entry {n}");
    }
    // The contents comes first.
    assert!(b.find("Contents").unwrap() < b.find("md_h0\"/>").unwrap_or(usize::MAX).min(b.find("Alpha").unwrap()));
}

#[test]
fn contents_options_are_validated_and_can_drop_the_title() {
    let none = with_doc(|d| d.table_of_contents = Some(TableOfContents::new().title("").max_level(1)));
    let text = visible_text(&body_with(OUTLINE, &none));
    assert!(text.starts_with("Alpha"), "{text}");
    assert_eq!(text.matches("Beta").count(), 1, "level 2 is not listed at max_level 1");
    let bad = with_doc(|d| d.table_of_contents = Some(TableOfContents::new().max_level(7)));
    assert!(matches!(
        markdown_to_docx("# x", &bad),
        Err(md2docx::CrateError::InvalidConfig(_))
    ));
}

#[test]
fn numbered_headings_use_word_multilevel_numbering_and_the_contents_repeats_the_numbers() {
    let cfg = with_doc(|d| {
        d.number_headings = true;
        d.table_of_contents = Some(TableOfContents::new().max_level(4));
    });
    let docx = markdown_to_docx(OUTLINE, &cfg).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert!(
        body.matches("<w:numPr>").count() >= 5,
        "every Markdown heading carries the numbering"
    );
    let numbering = zip_text(&docx, "word/numbering.xml");
    assert!(numbering.contains("%1.%2.%3"), "{numbering}");
    let text = visible_text(&body);
    for label in ["1 Alpha", "1.1 Beta", "1.1.1 Gamma", "1.1.1.1 Delta", "2 Epsilon"] {
        assert!(text.contains(label), "contents line `{label}` in {text}");
    }
    let plain = body_with(OUTLINE, &Config::default());
    assert!(!plain.contains("<w:numPr>"), "off by default");
}
