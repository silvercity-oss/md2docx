//! Identifier numbering is per conversion, not per process.

mod common;

use common::{zip_entries, zip_text};
use silvercity_oss::{markdown_to_docx, Config};

const DOC: &str = "# Title\n\nA [link](https://example.com/a) and [another](https://example.com/b)\nwith a note.[^1]\n\n[^1]: The note.\n";

fn convert() -> Vec<u8> {
    markdown_to_docx(DOC, &Config::native_math()).expect("conversion succeeds")
}

#[test]
fn identifiers_do_not_depend_on_earlier_conversions() {
    let first = convert();
    // Other work in the same process, including on other threads, must not move the numbering.
    let noisy: Vec<_> = (0..4)
        .map(|_| std::thread::spawn(|| (0..5).for_each(|_| drop(convert()))))
        .collect();
    for t in noisy {
        t.join().unwrap();
    }
    let second = convert();

    for part in [
        "word/document.xml",
        "word/_rels/document.xml.rels",
        "word/footnotes.xml",
    ] {
        assert_eq!(
            zip_text(&first, part),
            zip_text(&second, part),
            "{part} differs between runs"
        );
    }
    assert_eq!(zip_entries(&first), zip_entries(&second));
}

#[test]
fn numbering_restarts_at_one_for_every_document() {
    let rels = zip_text(&convert(), "word/_rels/document.xml.rels");
    assert!(rels.contains("rIdHyperlink1"), "{rels}");
    assert!(!rels.contains("rIdHyperlink3"), "only two links exist: {rels}");
}
