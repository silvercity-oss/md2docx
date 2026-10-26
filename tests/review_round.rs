//! Regression tests for the findings of the whole-project code review.

mod common;

use std::time::{Duration, Instant};

use common::*;
use md2docx::{convert, markdown_to_docx, Config, LossPolicy};

fn codes(md: &str, cfg: &Config) -> Vec<String> {
    convert(md, cfg)
        .unwrap()
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

// ---- footnotes

#[test]
fn every_footnote_has_its_number_even_with_tables_and_breaks() {
    let md = "A[^a] B[^b] C[^c]\n\n[^a]: Note with  \n    a hard break.\n\n[^b]: | x | y |\n    |---|---|\n    | 1 | 2 |\n\n[^c]: plain\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let notes = zip_text(&docx, "word/footnotes.xml");
    assert_eq!(notes.matches("<w:footnoteRef").count(), 3, "{notes}");
}

#[test]
fn lists_in_footnotes_are_numbered_and_declared() {
    // A footnote-only bullet list needs the numbering part; an ordered one needs its own number.
    let md = "Body.\n\n1. first\n2. second\n\nNote[^1] and[^2].\n\n[^1]: Steps:\n\n    1. one\n    2. two\n\n[^2]: Points:\n\n    - a\n    - b\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let doc = zip_text(&docx, "word/document.xml");
    let notes = zip_text(&docx, "word/footnotes.xml");
    let numbering = zip_text(&docx, "word/numbering.xml");
    let id_of = |xml: &str| -> Vec<String> {
        xml.split("<w:numId w:val=\"")
            .skip(1)
            .map(|c| c.split('"').next().unwrap().to_string())
            .collect()
    };
    let body_ids = id_of(&doc);
    let note_ids = id_of(&notes);
    assert!(!note_ids.is_empty());
    for id in body_ids.iter().chain(&note_ids) {
        assert!(
            numbering.contains(&format!("<w:num w:numId=\"{id}\"")),
            "numbering {id} is not defined: {numbering}"
        );
    }
    let ordered_note = &note_ids[0];
    assert!(
        !body_ids.contains(ordered_note),
        "the footnote's ordered list shares the body's number"
    );

    let only = markdown_to_docx("Note[^1].\n\n[^1]: Points:\n\n    - a\n    - b\n", &Config::default()).unwrap();
    assert!(zip_text(&only, "word/_rels/document.xml.rels").contains("numbering.xml"));
}

#[test]
fn a_large_footnote_is_linear_and_stops_at_the_deadline() {
    let code = "line\n".repeat(6_000);
    let md = format!(
        "Note[^1].\n\n[^1]: Code:\n\n    ```\n{}    ```\n",
        code.replace('\n', "\n    ").trim_end()
    );
    let started = Instant::now();
    let r = markdown_to_docx(&md, &Config::default());
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
    assert!(r.is_ok() || r.is_err()); // either answer is fine; taking forever is not
}

#[test]
fn an_html_anchor_is_emitted_once_however_many_footnote_references() {
    let md = "A[^1] B[^1] C[^1] [go](#t)\n\n<span id=\"t\">target</span>\n\n[^1]: <span id=\"t\">again</span>\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let all = format!(
        "{}{}",
        zip_text(&docx, "word/document.xml"),
        zip_text(&docx, "word/footnotes.xml")
    );
    assert_eq!(all.matches("<w:bookmarkStart").count(), 1, "{all}");
}

#[test]
fn undefined_footnote_marks_are_information_not_a_failure() {
    let md = "Use the pattern [^a-z] to match.\n";
    let cfg = Config::default().with_loss_policy(LossPolicy::Reject);
    assert!(convert(md, &cfg).is_ok());
    assert!(codes(md, &Config::default()).contains(&"footnote.undefined".to_string()));
}

// ---- lists and tables

#[test]
fn lists_inside_html_table_cells_keep_their_markers() {
    let md = "<table><tr><td>\n\n1. one\n2. two\n\n</td><td><ul><li>a</li><li>b</li></ul></td></tr></table>\n\nafter\n";
    let b = body_with(md, &Config::native_math());
    assert_well_formed(&b);
    assert_eq!(b.matches("<w:numPr>").count(), 4, "{b}");
    let after = b.rsplit("after").nth(1);
    assert!(after.is_some());
    // The paragraph after the table is not a list item.
    let tail = &b[b.rfind("<w:p ").unwrap()..];
    assert!(!tail.contains("<w:numPr>"), "{tail}");
}

#[test]
fn nested_html_tables_keep_the_outer_cells() {
    let md = "<table><tr><td>left<table><tr><td>inner</td></tr></table>tail</td><td>right</td></tr></table>\n";
    let b = body_with(md, &Config::native_math());
    assert_well_formed(&b);
    assert_eq!(b.matches("<w:tbl>").count(), 2, "{b}");
    let text = visible_text(&b);
    for want in ["left", "inner", "tail", "right"] {
        assert!(text.contains(want), "{want} missing in {text}");
    }
    // `right` is in the outer table's second cell, not a loose paragraph after it.
    let outer_end = b.rfind("</w:tbl>").unwrap();
    assert!(b[..outer_end].contains("right"), "{b}");
}

#[test]
fn a_list_item_that_starts_with_a_code_block_keeps_its_marker() {
    let md = "3. ```\n   code\n   ```\n4. next\n";
    let b = body_with(md, &Config::native_math());
    assert_eq!(b.matches("<w:numPr>").count(), 2, "{b}");
    assert!(b.find("<w:numPr>").unwrap() < b.find("code").unwrap(), "{b}");
}

// ---- headings and links

#[test]
fn heading_names_ignore_picture_alt_text_and_use_the_formula_source() {
    let md = "# The $E=mc^2$ formula\n\n## ![badge](x.png) Install\n\n## Install\n\n[a](#the-emc2-formula) [b](#install) [c](#install-1)\n";
    let r = convert(md, &Config::default()).unwrap();
    assert!(
        !r.diagnostics.iter().any(|d| d.code.as_str() == "link.anchor-missing"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn a_heading_that_is_only_an_equation_keeps_its_bookmark() {
    let md = "# $$x^2$$\n\n[top](#)\n\n[x](#x2)\n";
    let docx = markdown_to_docx(
        md,
        &Config::default().with_document({
            let mut d = md2docx::DocumentOptions::default();
            d.table_of_contents = Some(Default::default());
            d
        }),
    )
    .unwrap();
    let doc = zip_text(&docx, "word/document.xml");
    assert!(
        doc.contains("<w:bookmarkStart") && doc.contains("<w:bookmarkEnd"),
        "{doc}"
    );
}

#[test]
fn spaces_in_a_link_are_encoded_and_a_bad_scheme_is_refused() {
    let md = "[a](<https://a.example/my file.pdf>) [b](<java script:alert(1)>)\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(rels.contains("https://a.example/my%20file.pdf"), "{rels}");
    assert!(!rels.contains("alert"), "{rels}");
}

#[test]
fn shortening_a_mailto_link_is_reported() {
    let c = codes(
        "[m](mailto:a@b.example?subject=Hi&attach=/etc/passwd)\n",
        &Config::default(),
    );
    assert!(c.contains(&"link.modified".to_string()), "{c:?}");
    let c = codes("[m](mailto:a@b.example?subject=Hi)\n", &Config::default());
    assert!(!c.contains(&"link.modified".to_string()), "{c:?}");
}

// ---- input

#[test]
fn carriage_return_line_endings_do_not_swallow_lines() {
    let md = "```\rfirst\rsecond\r```\rafter\r";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    for want in ["first", "second", "after"] {
        assert!(text.contains(want), "{want} missing in {text}");
    }
}

#[test]
fn common_named_entities_in_html_are_decoded() {
    let md = "<p>&copy; 2026 &mdash; Acme &middot; &hellip; &euro;5</p>\n";
    let docx = markdown_to_docx(md, &Config::default()).unwrap();
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(
        text.contains("\u{a9} 2026 \u{2014} Acme \u{b7} \u{2026} \u{20ac}5"),
        "{text}"
    );
}

#[test]
fn a_missing_picture_costs_the_picture_not_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::trusted().with_base_dir(dir.path());
    let r = convert("before ![gone](absent.png) after", &cfg).unwrap();
    assert!(r.diagnostics.iter().any(|d| d.code.as_str() == "image.unreadable"));
    let text = visible_text(&zip_text(&r.docx, "word/document.xml"));
    assert!(text.contains("before") && text.contains("after"), "{text}");
}
