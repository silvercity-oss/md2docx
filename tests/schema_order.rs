//! The children of the property elements appear in the order the WordprocessingML schema
//! requires (strict consumers and validators reject others; Word itself is lenient).

mod common;

use common::*;
use md2docx::{markdown_to_docx, Config, DocumentOptions};
use quick_xml::events::Event;
use quick_xml::Reader;

const ORDERS: &[(&str, &[&str])] = &[
    (
        "rPr",
        &[
            "rStyle",
            "rFonts",
            "b",
            "bCs",
            "i",
            "iCs",
            "caps",
            "smallCaps",
            "strike",
            "dstrike",
            "outline",
            "shadow",
            "emboss",
            "imprint",
            "noProof",
            "snapToGrid",
            "vanish",
            "webHidden",
            "color",
            "spacing",
            "w",
            "kern",
            "position",
            "sz",
            "szCs",
            "highlight",
            "u",
            "effect",
            "bdr",
            "shd",
            "fitText",
            "vertAlign",
            "rtl",
            "cs",
            "em",
            "lang",
            "eastAsianLayout",
            "specVanish",
            "oMath",
        ],
    ),
    (
        "pPr",
        &[
            "pStyle",
            "keepNext",
            "keepLines",
            "pageBreakBefore",
            "framePr",
            "widowControl",
            "numPr",
            "suppressLineNumbers",
            "pBdr",
            "shd",
            "tabs",
            "suppressAutoHyphens",
            "kinsoku",
            "wordWrap",
            "overflowPunct",
            "topLinePunct",
            "autoSpaceDE",
            "autoSpaceDN",
            "bidi",
            "adjustRightInd",
            "snapToGrid",
            "spacing",
            "ind",
            "contextualSpacing",
            "mirrorIndents",
            "suppressOverlap",
            "jc",
            "textDirection",
            "textAlignment",
            "textboxTightWrap",
            "outlineLvl",
            "divId",
            "cnfStyle",
            "rPr",
            "sectPr",
            "pPrChange",
        ],
    ),
    (
        "sectPr",
        &[
            "headerReference",
            "footerReference",
            "footnotePr",
            "endnotePr",
            "type",
            "pgSz",
            "pgMar",
            "paperSrc",
            "pgBorders",
            "lnNumType",
            "pgNumType",
            "cols",
            "formProt",
            "vAlign",
            "noEndnote",
            "titlePg",
            "textDirection",
            "bidi",
            "rtlGutter",
            "docGrid",
            "printerSettings",
        ],
    ),
    (
        "tcPr",
        &[
            "cnfStyle",
            "tcW",
            "gridSpan",
            "hMerge",
            "vMerge",
            "tcBorders",
            "shd",
            "noWrap",
            "tcMar",
            "textDirection",
            "tcFitText",
            "vAlign",
            "hideMark",
        ],
    ),
    (
        "tblPr",
        &[
            "tblStyle",
            "tblpPr",
            "tblOverlap",
            "bidiVisual",
            "tblStyleRowBandSize",
            "tblStyleColBandSize",
            "tblW",
            "jc",
            "tblCellSpacing",
            "tblInd",
            "tblBorders",
            "shd",
            "tblLayout",
            "tblCellMar",
            "tblLook",
        ],
    ),
    (
        "style",
        &[
            "name",
            "aliases",
            "basedOn",
            "next",
            "link",
            "autoRedefine",
            "hidden",
            "uiPriority",
            "semiHidden",
            "unhideWhenUsed",
            "qFormat",
            "locked",
            "personal",
            "rsid",
            "pPr",
            "rPr",
            "tblPr",
            "trPr",
            "tcPr",
            "tblStylePr",
        ],
    ),
    ("numPr", &["ilvl", "numId"]),
    (
        "lvl",
        &[
            "start",
            "numFmt",
            "lvlRestart",
            "pStyle",
            "isLgl",
            "suff",
            "lvlText",
            "lvlPicBulletId",
            "legacy",
            "lvlJc",
            "pPr",
            "rPr",
        ],
    ),
];

/// quick-xml 0.41 hands out `&[u8]` names and 0.42 `&str`; both are `AsRef<[u8]>`.
fn local(name: impl AsRef<[u8]>) -> String {
    let s = String::from_utf8_lossy(name.as_ref());
    s.rsplit(':').next().unwrap_or("").to_string()
}

/// Every out-of-order pair of children found in `xml`.
fn disorder(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<(String, Option<usize>)> = Vec::new();
    let mut bad = Vec::new();
    loop {
        let (name, opens, closes) = match reader.read_event() {
            Ok(Event::Start(e)) => (local(e.name().as_ref()), true, false),
            Ok(Event::Empty(e)) => (local(e.name().as_ref()), false, false),
            Ok(Event::End(_)) => (String::new(), false, true),
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        if closes {
            stack.pop();
            continue;
        }
        if let Some((parent, last)) = stack.last_mut() {
            if let Some((_, order)) = ORDERS.iter().find(|(p, _)| p == parent) {
                if let Some(at) = order.iter().position(|n| *n == name) {
                    if last.is_some_and(|l| l > at) {
                        bad.push(format!("<{name}> after a later sibling inside <{parent}>"));
                    }
                    *last = Some(at.max(last.unwrap_or(0)));
                }
            }
        }
        if opens {
            stack.push((name, None));
        }
    }
    bad
}

#[test]
fn property_children_follow_the_schema_order() {
    let md = std::fs::read_to_string("tests/fixtures/01-structure.md").unwrap()
        + "\n\n<center><u>centred</u></center>\n\nH ~2~ O and x ^2^\n\n- [ ] task\n\n> quote\n";
    let mut doc = DocumentOptions::default();
    doc.number_headings = true;
    doc.page_numbers = true;
    let cfg = Config::native_math()
        .with_document(doc)
        .with_superscript_subscript(true);
    let docx = markdown_to_docx(&md, &cfg).unwrap();
    for part in [
        "word/document.xml",
        "word/styles.xml",
        "word/numbering.xml",
        "word/footnotes.xml",
    ] {
        let xml = zip_text(&docx, part);
        let bad = disorder(&xml);
        assert!(bad.is_empty(), "{part}: {:?}", &bad[..bad.len().min(5)]);
    }
    for entry in zip_entries(&docx).iter().filter(|e| e.starts_with("word/footer")) {
        let bad = disorder(&zip_text(&docx, entry));
        assert!(bad.is_empty(), "{entry}: {bad:?}");
    }
}

#[test]
fn the_checker_notices_a_wrong_order() {
    let bad = disorder("<w:pPr><w:rPr/><w:numPr><w:numId/><w:ilvl/></w:numPr></w:pPr>");
    assert_eq!(bad.len(), 2, "{bad:?}");
}
