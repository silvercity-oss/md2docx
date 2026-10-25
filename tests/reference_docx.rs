//! A reference `.docx` supplies the styles and theme of the output.

mod common;

use std::io::{Cursor, Write};

use common::*;
use silvercity_oss::{markdown_to_docx, Config, CrateError, ReferenceDocx};

/// A reference document: a converted one whose styles and theme we rewrite.
fn reference(styles: impl Fn(&str) -> String, theme: Option<&str>) -> Vec<u8> {
    let base = markdown_to_docx("# x\n", &Config::default()).unwrap();
    let mut zin = zip::ZipArchive::new(Cursor::new(&base)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        if f.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut bytes).unwrap();
        if name == "word/styles.xml" {
            bytes = styles(&String::from_utf8(bytes).unwrap()).into_bytes();
        }
        out.start_file(name, opts).unwrap();
        out.write_all(&bytes).unwrap();
    }
    if let Some(t) = theme {
        out.start_file("word/theme/theme1.xml", opts).unwrap();
        out.write_all(t.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

const THEME: &str = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Brand\"><a:themeElements/></a:theme>";

fn cfg_with(reference: &[u8]) -> Config {
    Config::default()
        .with_reference_docx(reference)
        .expect("a valid reference")
}

#[test]
fn the_reference_styles_win_and_missing_ones_are_filled_in() {
    let r = reference(
        |s| {
            // Brand colour on headings, and no CodeBlock / InlineCode style at all.
            let s = s.replace("1F3864", "AA0000");
            let start = s.find("<w:style w:type=\"paragraph\" w:styleId=\"CodeBlock\"").unwrap();
            let end = s[start..].find("</w:style>").unwrap() + start + "</w:style>".len();
            format!("{}{}", &s[..start], &s[end..])
        },
        None,
    );
    let docx = markdown_to_docx("# Title\n\n```\ncode\n```\n", &cfg_with(&r)).unwrap();
    let styles = zip_text(&docx, "word/styles.xml");
    assert_well_formed(&styles);
    assert!(
        styles.contains("AA0000") && !styles.contains("1F3864"),
        "the reference's colour is used"
    );
    assert!(
        styles.contains("w:styleId=\"CodeBlock\""),
        "a style the reference lacks is added"
    );
    assert_eq!(
        styles.matches("w:styleId=\"Heading1\"").count(),
        1,
        "no duplicate definitions"
    );
}

#[test]
fn the_theme_is_carried_over_with_its_content_type_and_relationship() {
    let r = reference(|s| s.to_string(), Some(THEME));
    let docx = markdown_to_docx("hello", &cfg_with(&r)).unwrap();
    assert!(zip_text(&docx, "word/theme/theme1.xml").contains("name=\"Brand\""));
    assert!(zip_text(&docx, "[Content_Types].xml").contains("/word/theme/theme1.xml"));
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(
        rels.contains("relationships/theme") && rels.contains("theme/theme1.xml"),
        "{rels}"
    );
    assert_well_formed(&rels);
    assert_well_formed(&zip_text(&docx, "[Content_Types].xml"));
    // Without a theme in the reference, none is written.
    let plain = markdown_to_docx("hello", &cfg_with(&reference(|s| s.to_string(), None))).unwrap();
    assert!(!zip_entries(&plain).iter().any(|n| n.contains("theme")));
}

#[test]
fn bad_references_are_refused_up_front() {
    assert!(matches!(
        Config::default().with_reference_docx(b"definitely not a zip"),
        Err(CrateError::InvalidConfig(m)) if m.contains("reference document")
    ));
    // A docx-shaped zip with broken styles.
    let broken = reference(|_| "<w:styles><w:style w:styleId=\"A\"></w:styles>".into(), None);
    assert!(ReferenceDocx::from_bytes(&broken).is_err());
    // A zip with no styles part at all.
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    out.start_file("word/document.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    out.write_all(b"<w:document/>").unwrap();
    let empty = out.finish().unwrap().into_inner();
    assert!(ReferenceDocx::from_bytes(&empty).is_err());
}

#[test]
fn an_entity_bomb_in_a_reference_is_refused() {
    let bomb = reference(
        |_| {
            "<!DOCTYPE s [<!ENTITY a \"aaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;\">]><w:styles>&b;</w:styles>"
                .into()
        },
        None,
    );
    assert!(ReferenceDocx::from_bytes(&bomb).is_err());
}

#[test]
fn a_styles_part_that_lies_about_its_size_is_cut_off() {
    // 5 MiB of padding inside a well-formed styles part: over the 4 MiB read limit.
    let big = reference(
        |s| {
            s.replace(
                "</w:styles>",
                &format!("<!--{}--></w:styles>", "x".repeat(5 * 1024 * 1024)),
            )
        },
        None,
    );
    assert!(ReferenceDocx::from_bytes(&big).is_err());
}
