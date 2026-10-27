//! Footnotes carry what the body carries: links, pictures, diagrams, equations and tables.

mod common;

use common::*;
use md2docx::{convert, AssetMap, Config, LossPolicy};

fn cfg() -> Config {
    let mut assets = AssetMap::new();
    assets.insert("fig", png_bytes(40, 20));
    Config::default()
        .with_image_resolver(assets)
        .with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, md2docx::FenceError> {
            Ok(png_bytes(30, 30))
        })
        .with_loss_policy(LossPolicy::Reject)
}

const MD: &str = "Text[^a] more[^b] and[^c].\n\n# Heading\n\n\
[^a]: See [the site](https://example.com/x) and [heading](#heading). ![A figure](fig)\n\n\
[^b]: An equation $x^2 + y^2$ and display:\n\n    $$a = \\frac{b}{c}$$\n\n\
[^c]: Intro\n\n    | h1 | h2 |\n    |----|----|\n    | 1  | 2  |\n\n    ```chart\n    1 2 3\n    ```\n";

#[test]
fn footnotes_keep_links_pictures_equations_diagrams_and_tables_without_findings() {
    let c = convert(MD, &cfg()).expect("nothing is lost, so strict mode accepts it");
    assert!(c.diagnostics.is_empty(), "{:#?}", c.diagnostics);
    let notes = zip_text(&c.docx, "word/footnotes.xml");
    assert_well_formed(&notes);
    assert!(notes.contains("<w:hyperlink"), "{notes}");
    assert!(notes.contains("<w:drawing>"));
    assert!(notes.contains("<w:tbl>"));
    assert!(notes.contains("<m:oMath"), "equations in footnotes are native OMML");
    assert!(!notes.contains("MD2DOCXOMML"), "no placeholder may survive");
    let body = zip_text(&c.docx, "word/document.xml");
    assert!(!body.contains("MD2DOCXOMML"));
    assert!(!body.contains("<m:oMath"), "the equations belong to the footnotes");
    assert_eq!((c.stats.footnotes, c.stats.tables), (3, 1));
}

#[test]
fn the_footnotes_relationship_part_declares_every_target() {
    let c = convert(MD, &cfg()).unwrap();
    let rels = zip_text(&c.docx, "word/_rels/footnotes.xml.rels");
    assert_well_formed(&rels);
    assert!(
        rels.contains("https://example.com/x") && rels.contains("TargetMode=\"External\""),
        "{rels}"
    );
    assert!(rels.contains("media/"), "{rels}");
    // Every relationship id the part uses is declared in the part's own rels.
    let notes = zip_text(&c.docx, "word/footnotes.xml");
    for attr in ["r:id=\"", "r:embed=\""] {
        for (i, _) in notes.match_indices(attr) {
            let rest = &notes[i + attr.len()..];
            let id = &rest[..rest.find('"').unwrap()];
            assert!(rels.contains(&format!("Id=\"{id}\"")), "{id} is not declared in {rels}");
        }
    }
    // The media files referenced exist in the package.
    for (i, _) in rels.match_indices("Target=\"media/") {
        let rest = &rels[i + "Target=\"".len()..];
        let target = &rest[..rest.find('"').unwrap()];
        assert!(zip_entries(&c.docx).contains(&format!("word/{target}")), "{target}");
    }
}

#[test]
fn a_document_without_footnote_extras_has_no_footnote_rels_part() {
    let c = convert("a[^1]\n\n[^1]: plain note\n", &Config::default()).unwrap();
    assert!(!zip_entries(&c.docx).iter().any(|n| n.ends_with("footnotes.xml.rels")));
}

#[test]
fn a_footnote_referenced_twice_shares_its_picture_and_charges_it_once() {
    let md = "a[^1] b[^1]\n\n[^1]: ![F](fig)\n";
    let c = convert(md, &cfg()).unwrap();
    assert_eq!(media_count(&c.docx), 1);
    let mut tight = cfg();
    tight.limits.max_total_decode_pixels = 800; // exactly one 40x20 decode
    assert!(convert(md, &tight).is_ok(), "the repeat reuses the first decode");
}

#[test]
fn footnote_images_obey_the_same_limits_as_body_images() {
    let mut c = cfg();
    c.limits.max_image_pixels = 100;
    assert!(convert("a[^1]\n\n[^1]: ![F](fig)\n", &c).is_err());
}

#[test]
fn references_inside_footnotes_are_still_left_as_text() {
    let c = convert("a[^1]\n\n[^1]: see [^2]\n\n[^2]: other\n", &Config::default()).unwrap();
    assert!(c.diagnostics.iter().any(|d| d.code == md2docx::Code::FootnoteNested));
}
