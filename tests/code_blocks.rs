//! Code block layouts: one paragraph per line, or the whole block as one paragraph.

mod common;

use common::*;
use silvercity_oss::{markdown_to_docx, CodeBlockLayout, Config, DocumentOptions, ImageAccess};

fn cfg(layout: CodeBlockLayout) -> Config {
    let mut doc = DocumentOptions::default();
    doc.code_blocks = layout;
    Config::native_math()
        .with_image_access(ImageAccess::None)
        .with_document(doc)
}

fn single() -> Config {
    cfg(CodeBlockLayout::SingleParagraph)
}

fn lines() -> Config {
    cfg(CodeBlockLayout::LinePerParagraph)
}

/// The code paragraphs of a document: for each `<w:p>` with the CodeBlock style, its XML.
fn code_paragraphs(doc: &str) -> Vec<&str> {
    doc.split("<w:p ")
        .skip(1)
        .filter(|p| p.contains("w:val=\"CodeBlock\""))
        .map(|p| p.split("</w:p>").next().unwrap())
        .collect()
}

/// What a reader copying the paragraph gets: text, tabs and line breaks in order.
fn paragraph_text(p: &str) -> String {
    let mut out = String::new();
    let mut rest = p;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        if rest.starts_with("<w:tab/>") {
            out.push('\t');
        } else if rest.starts_with("<w:br") {
            out.push('\n');
        } else if rest.starts_with("<w:t>") || rest.starts_with("<w:t ") {
            let gt = rest.find('>').unwrap();
            let end = rest.find("</w:t>").unwrap();
            out.push_str(&rest[gt + 1..end]);
            rest = &rest[end..];
            continue;
        }
        rest = &rest[1..];
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The code the document would give back, whichever layout it used.
fn copied_text(doc: &str) -> String {
    code_paragraphs(doc)
        .iter()
        .map(|p| paragraph_text(p))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fenced(code: &str) -> String {
    format!("```text\n{code}\n```\n")
}

const SAMPLES: &[&str] = &[
    "fn main() {\n    println!(\"hi\");\n}",
    "\tindented with a tab\n\t\tand two\nmixed \t tab",
    "  leading and trailing spaces  \n    ",
    "line one\n\nline three after a blank\n\n\nand two blanks",
    "<w:p> & \"q\" 'a' ]]> <![CDATA[x]]> &amp; &lt;",
    "unicode: é 日本語 \u{1F600} \u{05D0}\u{05D1} \u{202E}rtl\u{202C}",
    "trailing backslash \\\nnext",
    "x",
];

#[test]
fn both_layouts_give_back_exactly_the_source() {
    for code in SAMPLES {
        for (name, c) in [("lines", lines()), ("single", single())] {
            let doc = body_with(&fenced(code), &c);
            assert_well_formed(&doc);
            // Trailing spaces on the last line survive; a final newline is not part of the code.
            assert_eq!(copied_text(&doc), *code, "[{name}] {code:?}\n{doc}");
        }
    }
}

#[test]
fn single_paragraph_is_one_shaded_paragraph_with_line_breaks() {
    let code = "a\nb\n\nd\n\te";
    let doc = body_with(&fenced(code), &single());
    let paras = code_paragraphs(&doc);
    assert_eq!(paras.len(), 1, "{doc}");
    assert_eq!(paras[0].matches("<w:br").count(), 4);
    assert_eq!(paras[0].matches("<w:tab/>").count(), 1);
    assert_eq!(doc.matches("w:fill=\"F2F2F2\"").count(), 1);

    let doc = body_with(&fenced(code), &lines());
    assert_eq!(code_paragraphs(&doc).len(), 5);
    assert_eq!(doc.matches("<w:br").count(), 0);
}

#[test]
fn the_default_layout_is_unchanged() {
    assert_eq!(CodeBlockLayout::default(), CodeBlockLayout::LinePerParagraph);
    assert_eq!(
        DocumentOptions::default().code_blocks,
        CodeBlockLayout::LinePerParagraph
    );
    assert_eq!(
        code_paragraphs(&body_with(&fenced("a\nb"), &Config::default())).len(),
        2
    );
}

#[test]
fn blocks_are_separate_paragraphs_from_each_other_and_from_prose() {
    let md = "before\n\n```\none\ntwo\n```\n\nbetween\n\n```\nthree\n```\n\nafter\n";
    let doc = body_with(md, &single());
    assert_eq!(code_paragraphs(&doc).len(), 2);
    let text = visible_text(&doc);
    assert!(text.contains("beforeonetwobetweenthreeafter"), "{text}");
}

#[test]
fn single_paragraph_blocks_keep_their_indent_in_lists_and_quotes() {
    let doc = body_with(
        "- item\n\n  ```\n  a\n  b\n  ```\n\n> ```\n> c\n> d\n> ```\n",
        &single(),
    );
    let paras = code_paragraphs(&doc);
    assert_eq!(paras.len(), 2, "{doc}");
    assert!(paras[0].contains("w:left=\"720\""), "{}", paras[0]);
    assert!(paras[1].contains("w:left=\"360\""), "{}", paras[1]);
}

#[test]
fn html_pre_and_indented_code_follow_the_layout() {
    let doc = body_with("<pre>x\n  y\nz</pre>\n\n    indented\n    block\n", &single());
    let paras = code_paragraphs(&doc);
    assert_eq!(paras.len(), 2, "{doc}");
    assert_eq!(paragraph_text(paras[0]), "x\n  y\nz");
    assert_eq!(paragraph_text(paras[1]), "indented\nblock");
}

#[test]
fn control_characters_and_markup_never_reach_the_xml_as_markup() {
    let hostile = "a\u{0}b\u{1}c\u{8}d\u{b}e\u{c}f\u{1f}g\n</w:t></w:r></w:p><w:evil/>\n<?xml?><!--";
    for c in [lines(), single()] {
        // NUL is refused outright, as everywhere else; the other control characters are stripped.
        assert!(markdown_to_docx(&fenced(hostile), &c).is_err());
        let without_nul = hostile.replace('\u{0}', "");
        let doc = body_with(&fenced(&without_nul), &c);
        assert_well_formed(&doc);
        assert_only_xml_chars(&doc);
        assert!(!doc.contains("<w:evil"), "{doc}");
        assert!(copied_text(&doc).contains("</w:t></w:r></w:p><w:evil/>"));
    }
}

#[test]
fn long_blocks_are_split_into_paragraphs_of_500_lines() {
    // LibreOffice lays out a paragraph in time that grows with the square of its lines
    // (100k lines took 12 minutes), so one paragraph never holds more than 500.
    let code = "let x = 1;\n".repeat(1200);
    let doc = body_with(&fenced(code.trim_end()), &single());
    let paras = code_paragraphs(&doc);
    assert_eq!(paras.len(), 3, "{}", paras.len());
    assert_eq!(paras[0].matches("<w:br").count(), 499);
    assert_eq!(paras[2].matches("<w:br").count(), 199);
    assert_eq!(copied_text(&doc), code.trim_end());
    let exactly = "x\n".repeat(500);
    assert_eq!(
        code_paragraphs(&body_with(&fenced(exactly.trim_end()), &single())).len(),
        1
    );
}

#[test]
fn a_huge_single_paragraph_block_is_fine() {
    let code = "let x = 1;\n".repeat(150_000);
    let code = code.trim_end();
    let docx = markdown_to_docx(&fenced(code), &single()).unwrap();
    let doc = zip_text(&docx, "word/document.xml");
    assert_eq!(code_paragraphs(&doc).len(), 300);
    assert_eq!(doc.matches("<w:br").count(), 150_000 - 300);
    // The untrusted limits apply as usual: this many lines is refused, not built.
    let mut u = Config::untrusted();
    u.document.code_blocks = CodeBlockLayout::SingleParagraph;
    assert!(markdown_to_docx(&fenced(code), &u).is_ok());
    let more = "a\n".repeat(1_000_000);
    assert!(matches!(
        markdown_to_docx(&fenced(&more), &u),
        Err(silvercity_oss::CrateError::LimitExceeded(m)) if m.contains("max_elements")
    ));
}
