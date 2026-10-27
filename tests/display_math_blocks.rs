//! A display equation that is alone in its paragraph becomes a real `m:oMathPara`; the paragraph
//! it replaces must keep its list numbering and indentation.

mod common;

use common::*;
use md2docx::{markdown_to_docx, Config};

fn body(md: &str) -> String {
    zip_text(
        &markdown_to_docx(md, &Config::native_math()).unwrap(),
        "word/document.xml",
    )
}

#[test]
fn a_display_equation_inside_a_list_item_keeps_the_list_marker() {
    let b = body("1. First\n2. $$E = mc^2$$\n3. Third\n");
    let para = b.split("<m:oMathPara>").next().unwrap();
    let before = para.rsplit("<w:p>").next().unwrap();
    assert!(
        before.contains("<w:numPr>"),
        "the equation's paragraph lost its numbering: {before}"
    );
    assert_eq!(b.matches("<m:oMathPara>").count(), 1);
}

#[test]
fn a_display_equation_in_a_block_quote_keeps_its_indent() {
    let b = body("> $$a^2 + b^2 = c^2$$\n");
    let before = b.split("<m:oMathPara>").next().unwrap().rsplit("<w:p>").next().unwrap();
    assert!(before.contains("<w:ind "), "quote indent lost: {before}");
}

#[test]
fn a_top_level_display_equation_is_unchanged() {
    let b = body("Text\n\n$$x$$\n");
    // A paragraph with nothing to keep has no `pPr` at all.
    assert!(b.contains("<w:p><m:oMathPara><m:oMath>"), "{b}");
    assert!(!b.contains("<w:numPr>") && !b.contains("<w:ind "), "{b}");
}

#[test]
fn a_task_item_with_a_display_equation_keeps_its_checkbox() {
    let b = body("- [x] $$y = 1$$\n");
    assert!(b.contains('\u{2611}'), "the checkbox glyph was lost: {b}");
}
