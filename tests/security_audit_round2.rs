//! Regression tests for the findings of the second security audit.

mod common;

use std::time::{Duration, Instant};

use common::*;
use md2docx::{markdown_to_docx, Config, CrateError, Limits};

fn timed<T>(limit: Duration, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let out = f();
    assert!(started.elapsed() < limit, "took {:?}", started.elapsed());
    out
}

#[test]
fn a_footnote_with_an_equation_cannot_be_multiplied_without_bound() {
    // One 4 KiB formula referenced thousands of times used to grow footnotes.xml to gigabytes.
    let formula = format!("${}y$", "x^2+".repeat(1_000));
    let mut md = String::from("Text");
    for _ in 0..4_000 {
        md.push_str("[^f]");
    }
    md.push_str(&format!("\n\n[^f]: {formula}\n"));
    let r = timed(Duration::from_secs(60), || markdown_to_docx(&md, &Config::default()));
    assert!(
        matches!(r, Err(CrateError::LimitExceeded(_))),
        "{:?}",
        r.map(|d| d.len())
    );
}

#[test]
fn many_unclosed_aligned_html_blocks_are_linear() {
    // Quadratic in the number of tags, with no deadline check inside a block, before the fix.
    let md = format!("{}{}\n", "<center>".repeat(100_000), "</div>".repeat(100_000));
    let r = timed(Duration::from_secs(10), || markdown_to_docx(&md, &Config::default()));
    assert!(r.is_ok() || matches!(r, Err(CrateError::LimitExceeded(_))));
}

#[test]
fn repeated_html_anchor_ids_are_linear() {
    let mut md = String::new();
    for i in 0..9_998 {
        md.push_str(&format!("<a id=\"x{i}\"></a>\n\n"));
    }
    for _ in 0..60_000 {
        md.push_str("<a id=\"same\"></a>\n\n");
    }
    let r = timed(Duration::from_secs(20), || markdown_to_docx(&md, &Config::default()));
    assert!(r.is_ok() || matches!(r, Err(CrateError::LimitExceeded(_))));
}

#[test]
fn xml_noncharacters_in_a_link_target_cannot_corrupt_the_package() {
    // `&#xFFFF;` is decoded by the Markdown parser after the input was cleaned.
    let md = "[a](https://example.com/&#xFFFF;) and [b](https://example.com/&#xFFFE;x) and <https://e.org/ok>\n\nN[^1]\n\n[^1]: [c](https://c.example/&#xFFFF;)\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    for part in [
        "word/document.xml",
        "word/_rels/document.xml.rels",
        "word/footnotes.xml",
    ] {
        let xml = zip_text(&docx, part);
        assert_well_formed(&xml);
        assert_only_xml_chars(&xml);
    }
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(rels.contains("https://e.org/ok"), "{rels}");
    assert!(
        !rels.contains("example.com"),
        "the unusable targets are dropped: {rels}"
    );
}

#[test]
fn paragraph_ids_do_not_depend_on_earlier_conversions_on_the_thread() {
    let fresh = markdown_to_docx("hello\n\nworld", &Config::default()).unwrap();
    // A conversion that fails part-way must leave nothing behind.
    let mut tiny = Limits::default();
    tiny.max_elements = 50;
    let failing = "para\n\n".repeat(500);
    let r = markdown_to_docx(&failing, &Config::default().with_limits(tiny));
    assert!(matches!(r, Err(CrateError::LimitExceeded(_))));
    let after = markdown_to_docx("hello\n\nworld", &Config::default()).unwrap();
    assert_eq!(
        zip_text(&fresh, "word/document.xml"),
        zip_text(&after, "word/document.xml")
    );
    assert!(zip_text(&after, "word/document.xml").contains("w14:paraId=\"00000001\""));
}

#[test]
fn mailto_links_have_no_fragment_and_no_hidden_query_separators() {
    let md = "[a](mailto:a@b.example?subject=Hi#&attach=/etc/passwd) [b](mailto:a@b.example%3Fattach=/x) [c](mailto:a@b.example%26cc=x@y.example)\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(rels.contains("mailto:a@b.example?subject=Hi\""), "{rels}");
    assert!(
        !rels.contains('#') && !rels.contains("attach") && !rels.contains("%3F") && !rels.contains("%26"),
        "{rels}"
    );
}
