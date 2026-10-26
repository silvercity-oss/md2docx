//! Identifier numbering is per conversion, not per process.

mod common;

use common::{zip_entries, zip_text};
use md2docx::{markdown_to_docx, Config};

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

#[test]
fn a_fixed_timestamp_makes_the_whole_file_reproducible() {
    use std::time::{Duration, UNIX_EPOCH};
    let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000); // 2023-11-14T22:13:20Z
    let cfg = Config::native_math().with_timestamp(at);
    let first = markdown_to_docx(DOC, &cfg).unwrap();
    std::thread::sleep(Duration::from_millis(1100)); // the wall clock moves on
    let second = markdown_to_docx(DOC, &cfg).unwrap();
    assert_eq!(first, second, "identical input and timestamp must give identical bytes");
    let core = zip_text(&first, "docProps/core.xml");
    assert!(core.contains("2023-11-14T22:13:20Z"), "{core}");
    // Without one, the time of the conversion is used.
    let now = markdown_to_docx(DOC, &Config::native_math()).unwrap();
    assert!(!zip_text(&now, "docProps/core.xml").contains("2023-11-14"));
}

#[test]
fn an_impossible_timestamp_is_a_configuration_error() {
    use std::time::{Duration, UNIX_EPOCH};
    let far = UNIX_EPOCH + Duration::from_secs(300_000_000_000);
    assert!(matches!(
        markdown_to_docx(DOC, &Config::native_math().with_timestamp(far)),
        Err(md2docx::CrateError::InvalidConfig(_))
    ));
}
