//! The ordinary library default is the untrusted-input policy; trust is opt-in.

mod common;

use common::*;
use md2docx::{markdown_to_docx, Config, ImageAccess, Limits, MathMode};

#[test]
fn default_is_the_untrusted_policy() {
    let d = Config::default();
    assert_eq!(d.math.mode, MathMode::NativeOmml, "no TeX process by default");
    assert!(!d.graphs.enabled, "no diagram renderer process by default");
    assert_eq!(d.image_access, ImageAccess::None, "no local files by default");
    assert!(!d.html_links);
    let u = Config::untrusted();
    assert_eq!(d.limits.max_input_bytes, u.limits.max_input_bytes);
    assert_eq!(d.limits.max_output_bytes, u.limits.max_output_bytes);
    assert_eq!(d.limits.max_elements, u.limits.max_elements);
    assert_eq!(Limits::default().max_output_bytes, 50 * 1024 * 1024);
    assert!(Limits::trusted().max_output_bytes > Limits::default().max_output_bytes);
    assert!(d.validate().is_ok());
}

#[test]
fn default_reads_no_files_runs_no_tools_and_keeps_diagrams_as_code() {
    let dir = Fixture::new();
    let md = format!(
        "![alt text]({})\n\n<a href=\"https://example.com\">html link</a>\n\n```mermaid\ngraph TD; A-->B\n```\n\n$x^2$\n",
        dir.path("fixture.png").display().to_string().replace('\\', "/")
    );
    let docx = markdown_to_docx(&md, &Config::default()).expect("default config converts");
    assert_eq!(media_count(&docx), 0, "no image bytes were read from disk");
    let xml = zip_text(&docx, "word/document.xml");
    assert!(xml.contains("alt text"), "image becomes its alt text");
    assert!(xml.contains("graph TD"), "diagram source stays as code");
    assert!(xml.contains("<m:oMath"), "math is native");
    assert!(!xml.contains("w:hyperlink"), "html links are dropped");
}

#[test]
fn native_math_alone_is_not_the_untrusted_policy() {
    // `native_math()` only removes the TeX process. It is a trusted preset and must stay
    // distinguishable from the default so nobody mistakes one for the other.
    let n = Config::native_math();
    assert_eq!(n.math.mode, MathMode::NativeOmml);
    assert_eq!(n.image_access, ImageAccess::BaseDir);
    assert!(n.graphs.enabled);
    assert!(n.html_links);
    assert!(n.limits.max_output_bytes > Config::untrusted().limits.max_output_bytes);

    let t = Config::trusted();
    assert_eq!(t.math.mode, MathMode::ImageFallback);
}
