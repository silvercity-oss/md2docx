//! Opt-in Markdown/HTML conveniences and layout details: autolinks, super/subscript, HTML
//! anchors, content-sized table columns, mailto filtering.

mod common;

use common::*;
use silvercity_oss::{markdown_to_docx, Config};

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
    assert!((total as i64 - 9026).abs() < 400, "the table keeps the text width: {g:?}");
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
    assert!(inside.contains("b.example") && !inside.contains("a.example") && !inside.contains("c.example"), "{inside}");
}

#[test]
fn superscript_and_subscript_are_opt_in() {
    let md = "H ~2~ O and x ^2^ and ~~gone~~";
    let off = body_with(md, &Config::default());
    assert!(!off.contains("w:vertAlign"), "{off}");
    assert!(off.contains("<w:strike"), "single tildes stay strikethrough by default");
    let on = body_with(md, &Config::default().with_superscript_subscript(true));
    assert!(on.contains("w:val=\"subscript\"") && on.contains("w:val=\"superscript\""), "{on}");
    assert!(on.contains("<w:strike"), "double tildes still strike");
}

#[test]
fn html_anchors_are_link_targets() {
    let md = "<a name=\"intro\"></a>\n\nJump to [the intro](#intro) or <a href=\"#intro\">here</a>.\n\n<div id=\"box\">Box</div>\n\n[to box](#box) and [nowhere](#nope)\n";
    let b = body_with(md, &Config::default());
    assert!(b.contains("<w:bookmarkStart w:id=\"1048576\" w:name=\"md_a0\""), "{b}");
    assert!(b.matches("w:anchor=\"md_a0\"").count() == 2, "{b}");
    assert!(b.contains("w:anchor=\"md_a1\""), "the id of any element is a target: {b}");
    assert!(!b.contains("md_a2"));
    let diags = silvercity_oss::convert(md, &Config::default()).unwrap().diagnostics;
    assert!(diags.iter().any(|d| d.code == silvercity_oss::Code::LinkAnchorMissing), "{diags:?}");
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
