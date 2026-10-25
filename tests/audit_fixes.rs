//! Regression tests for the problems found by the pre-release audit.

mod common;

use std::time::{Duration, Instant};

use common::*;
use silvercity_oss::{markdown_to_docx, Config, CrateError, FenceError, ImageAccess};

fn fast(md: &str, cfg: &Config) -> Result<Vec<u8>, CrateError> {
    let started = Instant::now();
    let r = markdown_to_docx(md, cfg);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "took {:?} for {} bytes",
        started.elapsed(),
        md.len()
    );
    r
}

// ---- denial of service

#[test]
fn entity_runs_in_html_are_linear() {
    // 200 s each at 2 MiB before the fix.
    let amps = "&".repeat((2 << 20) - 200);
    assert!(fast(&format!("<div>\n{amps}\n"), &Config::untrusted()).is_ok());
    assert!(fast(&format!("x <img alt=\"{amps}\" src=\"a.png\">\n"), &Config::untrusted()).is_ok());
}

#[test]
fn script_openers_inside_an_attribute_are_linear() {
    let md = format!("x <b title=\"{}</script\">y\n", "<script ".repeat(262_000));
    assert!(fast(&md, &Config::untrusted()).is_ok());
}

#[test]
fn memory_is_bounded_by_an_element_budget() {
    // Each of these built gigabytes of objects before anything was checked.
    let u = Config::untrusted();
    let two_mib = 2 << 20;
    let shapes = [
        ("blank code lines", format!("```\n{}```\n", "\n".repeat(two_mib - 10))),
        ("code lines", format!("```\n{}```\n", "a\n".repeat(two_mib / 2 - 10))),
        ("paragraphs", "x\n\n".repeat(two_mib / 3)),
        ("quotes", ">x\n\n".repeat(two_mib / 4)),
        ("list items", "- x\n".repeat(two_mib / 4)),
        ("ordered/bullet lists", "1. x\n- y\n".repeat(two_mib / 9)),
        ("bold runs", "**x** ".repeat(two_mib / 6)),
        (
            "html table cells",
            format!("<table><tr>{}", "<td>".repeat(two_mib / 4 - 10)),
        ),
        ("html breaks", format!("<div>\n{}", "<br>".repeat(two_mib / 4 - 10))),
    ];
    for (name, md) in shapes {
        match fast(&md, &u) {
            Err(CrateError::LimitExceeded(m)) => assert!(m.contains("max_elements"), "[{name}] {m}"),
            other => panic!("[{name}] expected the element limit, got {:?}", other.map(|d| d.len())),
        }
    }
    // A big wide table counts its cells.
    let wide = format!(
        "{}|\n{}|\n|{}\n",
        "| h ".repeat(30_000),
        "|---".repeat(30_000),
        " x |".repeat(3)
    );
    assert!(matches!(fast(&wide, &u), Err(CrateError::LimitExceeded(_))));
}

#[test]
fn ordinary_documents_are_nowhere_near_the_budget() {
    let mut md = String::new();
    for i in 0..1500 {
        md.push_str(&format!("## Section {i}\n\nSome **bold** and *italic* text with `code` and a [link](https://e.com/{i}).\n\n- one\n- two\n\n"));
    }
    assert!(md.len() < 2 << 20);
    assert!(fast(&md, &Config::untrusted()).is_ok());
}

#[test]
fn the_element_budget_is_configurable() {
    let mut cfg = Config::native_math();
    cfg.limits.max_elements = 50;
    assert!(matches!(
        markdown_to_docx(&"para\n\n".repeat(100), &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("max_elements")
    ));
    cfg.limits.max_elements = 1000;
    assert!(markdown_to_docx(&"para\n\n".repeat(100), &cfg).is_ok());
    // Footnote bodies count too.
    cfg.limits.max_elements = 40;
    let notes: String = (0..30).map(|i| format!("a[^{i}] ")).collect::<String>()
        + "\n\n"
        + &(0..30).map(|i| format!("[^{i}]: note\n\n")).collect::<String>();
    assert!(matches!(
        markdown_to_docx(&notes, &cfg),
        Err(CrateError::LimitExceeded(_))
    ));
}

#[test]
fn deep_quotes_get_bounded_indents() {
    // 6 million `>` made 360 * depth overflow i32 (a panic in debug builds). The arithmetic is
    // covered by a unit test in parser.rs; this checks the document for a deep but quick case.
    let md = format!("{}\n```\nx\n```\n", ">".repeat(200_000));
    match fast(&md, &Config::native_math()) {
        Ok(docx) => {
            let doc = zip_text(&docx, "word/document.xml");
            for chunk in doc.split("w:left=\"").skip(1) {
                let v: i64 = chunk.split('"').next().unwrap().parse().unwrap();
                assert!((0..=31_680).contains(&v), "indent {v}");
            }
        }
        Err(CrateError::LimitExceeded(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

// ---- package validity

#[test]
fn links_in_footnotes_do_not_leave_dangling_relationships() {
    let md = "Text[^1].\n\n[^1]: see [the site](https://example.com/) here\n";
    for cfg in [Config::untrusted(), Config::native_math()] {
        let docx = markdown_to_docx(md, &cfg).unwrap();
        let notes = zip_text(&docx, "word/footnotes.xml");
        assert!(
            !notes.contains("r:id"),
            "footnote part must not reference relationships: {notes}"
        );
        assert!(visible_text(&notes).contains("the site"), "{notes}");
        assert!(notes.contains("Hyperlink"), "link text keeps the link look");
    }
}

#[test]
fn every_relationship_id_resolves() {
    let fx = Fixture::new();
    let cfg = Config::native_math().with_base_dir(fx.dir.path());
    let md = "[a](https://example.com/) ![p](fixture.png)\n\nNote[^1] and <a href=\"https://e.org/\">b</a>\n\n[^1]: [c](https://c.example/)\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    for part in ["word/document.xml", "word/footnotes.xml"] {
        let xml = zip_text(&docx, part);
        for chunk in xml.split("r:id=\"").skip(1).chain(xml.split("r:embed=\"").skip(1)) {
            let id = chunk.split('"').next().unwrap();
            assert!(
                rels.contains(&format!("Id=\"{id}\"")),
                "{part}: {id} has no relationship in {rels}"
            );
        }
    }
}

#[test]
fn a_markdown_table_inside_an_html_table_keeps_both() {
    let md = "<table><tr><td>\n\nLEFTCELL $x^2$\n\n</td><td>\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n</td></tr></table>\n\nafter\n";
    let b = body_with(md, &Config::native_math());
    assert_well_formed(&b);
    let text = visible_text(&b);
    assert!(text.contains("LEFTCELL") && text.contains("after"), "{text}");
    assert!(text.contains('a') && text.contains('1') && text.contains('2'));
    assert_eq!(b.matches("<w:tbl>").count(), 2, "outer and nested table");
    assert!(
        b.contains("</w:tbl><w:p "),
        "a cell must end with a paragraph after a nested table: {b}"
    );
    assert!(b.contains("<m:oMath>"));
    // Unclosed HTML table around a Markdown table, and the other way round.
    for md in [
        "<table><tr><td>\n\n| a |\n|---|\n| 1 |\n",
        "| a |\n|---|\n| 1 |\n\n<table><tr><td>x",
    ] {
        assert_well_formed(&body_with(md, &Config::native_math()));
    }
}

#[test]
fn list_start_numbers_stay_within_32_bits() {
    let b = body_with(
        "<ol start=\"18446744073709551615\"><li>x</li></ol>\n\n99999999999999999999. y\n",
        &Config::native_math(),
    );
    let docx = markdown_to_docx(
        "<ol start=\"18446744073709551615\"><li>x</li></ol>\n",
        &Config::native_math(),
    )
    .unwrap();
    let numbering = zip_text(&docx, "word/numbering.xml");
    for chunk in numbering.split("w:startOverride w:val=\"").skip(1) {
        let v: u64 = chunk.split('"').next().unwrap().parse().unwrap();
        assert!(v <= i32::MAX as u64, "{v}");
    }
    assert_well_formed(&b);
}

#[test]
fn archive_entries_are_not_marked_executable() {
    let docx = markdown_to_docx("x", &Config::untrusted()).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&docx)).unwrap();
    for i in 0..z.len() {
        let f = z.by_index(i).unwrap();
        if !f.is_dir() {
            assert_eq!(f.unix_mode().map(|m| m & 0o111), Some(0), "{:?}", f.name());
        }
    }
}

// ---- content fidelity

#[test]
fn html_attribute_lookup_ignores_text_inside_quoted_values() {
    let md = "<a title=\"href=https://evil.example/\" href=\"https://good.example/\">link</a>\n";
    let docx = markdown_to_docx(md, &Config::native_math()).unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(
        rels.contains("good.example") && !rels.contains("evil.example"),
        "{rels}"
    );
}

#[test]
fn a_replaced_fence_does_not_swallow_the_next_line() {
    let cfg = Config::default().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        Ok(png_bytes(40, 20))
    });
    for md in [
        "```chart\n1 2\n```\nNEXT para\n",
        "> ```chart\n> 1 2\n> ```\n> NEXT para\n",
        "- ```chart\n  1 2\n  ```\n- NEXT para\n",
    ] {
        let b = body_with(md, &cfg);
        assert_well_formed(&b);
        assert_eq!(b.matches("<w:drawing>").count(), 1, "{md:?}");
        let para = b
            .split("<w:p ")
            .find(|p| p.contains("NEXT para"))
            .unwrap_or_else(|| panic!("{md:?}: {b}"));
        assert!(
            !para.contains("<w:drawing>"),
            "{md:?}: the picture and the next line share a paragraph"
        );
    }
}

#[test]
fn footnote_labels_match_case_insensitively() {
    let docx = markdown_to_docx("See[^Note].\n\n[^note]: the body\n", &Config::native_math()).unwrap();
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(!text.contains("[^Note]"), "{text}");
    assert!(visible_text(&zip_text(&docx, "word/footnotes.xml")).contains("the body"));
}

#[test]
fn script_text_with_odd_whitespace_in_the_tag_stays_hidden() {
    let b = body_with(
        // (A `>` at the start of a line is a block quote, so `<script\r\n>` is not a tag to
        // CommonMark at all and its text really is text.)
        "a <script\u{c}>SECRET</script> b\n\na <script\t>SECRET2</script> b\n",
        &Config::native_math(),
    );
    let text = visible_text(&b);
    assert!(!text.contains("SECRET"), "{text}");
}

// ---- image paths

#[test]
fn image_paths_outside_the_base_get_the_same_answer_whether_or_not_they_exist() {
    let fx = Fixture::new();
    let cfg = Config::native_math().with_base_dir(fx.dir.path());
    let outside = [
        ("/etc/passwd", "/etc/definitely-not-here-xyz"),
        ("../../../../etc/passwd", "../../../../etc/definitely-not-here-xyz"),
        ("sub/../../outside.png", "sub/../../outside-missing.png"),
    ];
    for (exists, missing) in outside {
        let msg = |r: &str| match markdown_to_docx(&format!("![x](<{r}>)\n"), &cfg) {
            Err(CrateError::UnsafeContent(m)) => m.replace(r, "REF"),
            other => panic!("`{r}`: expected a refusal, got {:?}", other.map(|d| d.len())),
        };
        assert_eq!(msg(exists), msg(missing), "`{exists}` vs `{missing}` differ");
    }
    for unc in [
        "//host/share/x.png",
        // Inside `<...>` a Markdown backslash escapes the next character.
        r"\\\\host\\share\x.png",
        "%5C%5Chost%5Cshare%5Cx.png",
        "file:////host/share/x.png",
    ] {
        assert!(
            matches!(
                markdown_to_docx(&format!("![x](<{unc}>)\n"), &cfg),
                Err(CrateError::UnsafeContent(_))
            ),
            "{unc}"
        );
    }
    // Inside the base still works, with `..` that stays inside.
    std::fs::create_dir(fx.path("sub")).unwrap();
    assert!(markdown_to_docx("![x](sub/../fixture.png)\n", &cfg).is_ok());
    // An absolute path inside the base (not canonicalised: on Windows that gives `\\\\?\\C:`).
    // (Forward slashes: inside `<...>` a Markdown backslash would escape the next character.)
    let abs = fx.path("fixture.png").to_string_lossy().replace('\\', "/");
    assert!(markdown_to_docx(&format!("![x](<{abs}>)\n"), &cfg).is_ok());
    // And with access off nothing is read at all.
    let none = cfg.clone().with_image_access(ImageAccess::None);
    assert!(markdown_to_docx("![x](/etc/passwd)\n", &none).is_ok());
}

// ---- characters that are not XML

#[test]
fn non_xml_characters_are_refused_in_config_and_in_the_public_converter() {
    let mut doc = silvercity_oss::DocumentOptions::default();
    doc.body_font = "Cali\u{fffe}bri".into();
    assert!(matches!(
        markdown_to_docx("x", &Config::default().with_document(doc)),
        Err(CrateError::InvalidConfig(_))
    ));
    assert!(silvercity_oss::omml::latex_to_omml("a\u{1}b").is_err());
    assert!(silvercity_oss::omml::latex_to_omml("a\u{fffe}b").is_err());
    let e = silvercity_oss::omml::latex_to_omml("\\frac{").unwrap_err();
    assert!(!e.message().is_empty() && e.to_string() == e.message());
}

// ---- no scratch directory unless something is rendered to a file

#[cfg(unix)]
#[test]
fn untrusted_conversion_does_not_need_a_scratch_directory() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.md");
    std::fs::write(&input, "# Title\n\n$x^2$\n\n```mermaid\ngraph TD; A-->B\n```\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .arg(&input)
        .arg("--untrusted")
        .env("TMPDIR", "/nonexistent/scratch")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_replaced_fence_keeps_its_container() {
    // The blank line after the picture must not end the quote or the list item that the
    // following text belongs to.
    let cfg = Config::default().with_fence_renderer("chart", |_: &str| -> Result<Vec<u8>, FenceError> {
        Ok(png_bytes(40, 20))
    });
    let pre = |md: &str| silvercity_oss::__internals::preprocess(md, &cfg).unwrap().markdown;
    for md in [
        "> - a\n>   ```chart\n>   1\n>   ```\n>   NEXT\n> - b\n",
        "> 1. ```chart\n>    1\n>    ```\n>    NEXT\n",
        "- ```chart\n  1\n  ```\n  NEXT\n- b\n",
        "> ```chart\n> 1\n> ```\n> NEXT\n",
        "```chart\n1\n```\nNEXT\n",
    ] {
        let out = pre(md);
        // Every line of the rewritten document that is blank must carry the quote marker
        // when the original lines did, so no container ends early.
        let quoted = md.starts_with('>');
        for line in out.lines() {
            if quoted {
                assert!(
                    line.starts_with('>'),
                    "{md:?} became {out:?}: line {line:?} left the quote"
                );
            }
        }
        let b = body_with(md, &cfg);
        assert_well_formed(&b);
        assert_eq!(b.matches("<w:drawing>").count(), 1, "{md:?}");
    }
    // The text after the fence stays in the list item (it is indented in the output), not in
    // a new top-level paragraph.
    let out = pre("- ```chart\n  1\n  ```\n  NEXT\n- b\n");
    assert!(out.contains("\n  NEXT\n"), "{out:?}");
}
