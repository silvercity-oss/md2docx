//! Every example input from the CommonMark specification (652 of them) is
//! converted and checked. This is a robustness and no-content-lost check, not a
//! conformance test: the parser is pulldown-cmark, which has its own spec suite.
//!
//! For each input and for both the default and the untrusted configuration:
//! - the conversion succeeds and yields a zip whose `document.xml` is
//!   well-formed and contains only legal XML characters;
//! - no hyperlink target uses a script-capable scheme;
//! - every piece of text the parser reports (outside raw HTML) appears in the
//!   document's visible text, so nothing was silently dropped.
//!
//! The corpus lives in `tests/data` (CC BY-SA 4.0, see the README there) and is
//! not published with the crate; set `REQUIRE_SPEC_CORPUS=1` to fail, not skip,
//! when it is missing.

mod common;

use common::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use silvercity_oss::{markdown_to_docx, Config, ImageAccess};

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/commonmark-spec-examples.txt");

fn examples() -> Option<Vec<String>> {
    match std::fs::read_to_string(CORPUS) {
        Ok(text) => Some(text.split("<<<EXAMPLE>>>\n").skip(1).map(str::to_string).collect()),
        Err(e) => {
            assert!(
                std::env::var("REQUIRE_SPEC_CORPUS").map_or(true, |v| v.is_empty()),
                "REQUIRE_SPEC_CORPUS is set but {CORPUS} cannot be read: {e}"
            );
            eprintln!("skipping: spec corpus not present");
            None
        }
    }
}

/// The text the parser reports as document content, outside raw HTML.
fn expected_fragments(md: &str) -> Vec<String> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let mut out = Vec::new();
    let mut html_block = 0usize;
    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::HtmlBlock) => html_block += 1,
            Event::End(TagEnd::HtmlBlock) => html_block = html_block.saturating_sub(1),
            Event::Text(t) | Event::Code(t) if html_block == 0 => {
                let squeezed: String = t.chars().filter(|c| !c.is_whitespace()).collect();
                if !squeezed.is_empty() {
                    out.push(squeezed);
                }
            }
            _ => {}
        }
    }
    out
}

/// Inputs whose text is legitimately not all visible: the elements are removed
/// on purpose, or `$` makes the text a formula.
fn text_check_applies(md: &str) -> bool {
    let lower = md.to_ascii_lowercase();
    !(md.contains('$')
        || [
            "<script",
            "<style",
            "<iframe",
            "<object",
            "<embed",
            "<template",
            "<noscript",
            "<title",
            "<textarea",
        ]
        .iter()
        .any(|t| lower.contains(t)))
}

fn check(name: &str, md: &str, cfg: &Config) -> Result<(), String> {
    let docx = markdown_to_docx(md, cfg).map_err(|e| format!("conversion failed: {e}"))?;
    let doc = zip_text(&docx, "word/document.xml");
    std::panic::catch_unwind(|| {
        assert_well_formed(&doc);
        assert_only_xml_chars(&doc);
    })
    .map_err(|_| format!("[{name}] document.xml is malformed"))?;

    let rels = zip_text(&docx, "word/_rels/document.xml.rels").to_ascii_lowercase();
    for scheme in ["javascript:", "vbscript:", "data:", "file:"] {
        if rels.contains(scheme) {
            return Err(format!("[{name}] a hyperlink target uses `{scheme}`"));
        }
    }

    if text_check_applies(md) {
        let visible: String = visible_text(&doc).chars().filter(|c| !c.is_whitespace()).collect();
        for frag in expected_fragments(md) {
            if !visible.contains(&frag) {
                return Err(format!(
                    "[{name}] text `{frag}` was dropped; visible text is `{visible}`"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn every_spec_example_converts_cleanly() {
    let Some(all) = examples() else { return };
    assert_eq!(all.len(), 652, "the corpus should hold every CommonMark 0.31.2 example");
    let default = Config::native_math().with_image_access(ImageAccess::None);
    let untrusted = Config::untrusted();
    let mut failures = Vec::new();
    for (i, md) in all.iter().enumerate() {
        for (name, cfg) in [("default", &default), ("untrusted", &untrusted)] {
            if let Err(why) = check(name, md, cfg) {
                failures.push(format!("example {} {why}\n--- input ---\n{md}", i + 1));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n\n{}",
        failures.len(),
        failures.join("\n=====\n")
    );
}
