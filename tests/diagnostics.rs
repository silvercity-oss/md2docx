//! Structured findings: stable codes, severities, spans into the original Markdown, and the
//! strict policy that turns lossy conversions into errors.

mod common;

use std::sync::Arc;

use common::*;
use silvercity_oss::{
    analyze, convert, markdown_to_docx, AssetMap, Code, Config, CrateError, Diagnostic, FenceError, LossPolicy,
    Severity,
};

fn run(md: &str) -> Vec<Diagnostic> {
    convert(md, &Config::default()).expect("converts").diagnostics
}

fn only(diags: &[Diagnostic], code: Code) -> &Diagnostic {
    let hits: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(hits.len(), 1, "expected exactly one {code}, got {diags:#?}");
    hits[0]
}

/// The source text a diagnostic points at.
fn at<'a>(md: &'a str, d: &Diagnostic) -> &'a str {
    let s = d.span.as_ref().unwrap_or_else(|| panic!("{} has no span", d.code));
    &md[s.start..s.end]
}

#[test]
fn a_faithful_document_has_no_findings_and_reports_what_it_made() {
    let md = "# Title\n\nSome *text* with a [link](https://example.com) and `code`.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nNote.[^1]\n\n```rust\nfn main() {}\n```\n\n$x^2$\n\n[^1]: The note.\n";
    let c = convert(md, &Config::default().with_loss_policy(LossPolicy::Reject)).expect("faithful");
    assert!(c.diagnostics.is_empty(), "{:#?}", c.diagnostics);
    assert_eq!(
        (
            c.stats.headings,
            c.stats.tables,
            c.stats.footnotes,
            c.stats.code_blocks,
            c.stats.equations
        ),
        (1, 1, 1, 1, 1)
    );
    assert_eq!(c.stats.hyperlinks, 1);
}

#[test]
fn images_that_are_not_embedded_say_why() {
    let md = "a ![remote](https://example.com/x.png) b ![inline](data:image/png;base64,AAAA) c ![local](pic.png) d";
    let d = run(md);
    let remote: Vec<_> = d.iter().filter(|d| d.code == Code::ImageRemote).collect();
    assert_eq!(remote.len(), 2);
    assert_eq!(at(md, remote[0]), "![remote](https://example.com/x.png)");
    assert_eq!(at(md, only(&d, Code::ImageDisabled)), "![local](pic.png)");
    assert!(d.iter().all(|d| d.severity == Severity::Warning));
    let s = only(&d, Code::ImageDisabled).span.as_ref().unwrap();
    assert_eq!((s.line, s.column), (1, 82));
}

#[test]
fn unsupported_image_bytes_are_reported_even_when_the_host_supplied_them() {
    let mut assets = AssetMap::new();
    assets.insert("logo", b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec());
    let cfg = Config::default().with_image_resolver(assets);
    let md = "![logo](logo)";
    let d = convert(md, &cfg).unwrap().diagnostics;
    assert!(only(&d, Code::ImageUnsupported).message.contains("PNG, JPEG or GIF"));
}

#[test]
fn footnote_content_that_cannot_be_kept_is_reported_at_its_definition() {
    let md = "x[^1] y[^2] z[^3]\n\n[^1]: ![pic](pic.png)\n\n[^2]: see [site](https://example.com) and $q$\n\n[^3]: intro\n\n    | a | b |\n    |---|---|\n    | 1 | 2 |\n";
    let cfg = Config::native_math().with_image_access(silvercity_oss::ImageAccess::None);
    let d = convert(md, &cfg).unwrap().diagnostics;
    assert_eq!(at(md, only(&d, Code::FootnoteImage)), "![pic](pic.png)");
    assert!(only(&d, Code::FootnoteLink).span.is_some());
    assert_eq!(at(md, only(&d, Code::FootnoteMath)), "$q$");
    let table = only(&d, Code::FootnoteTable);
    assert!(at(md, table).starts_with("[^3]:"), "{}", at(md, table));
}

#[test]
fn footnote_references_without_a_home_are_reported() {
    let md = "Known[^a], unknown[^zz].\n\n[^a]: Has a body with a nested ref[^a].\n\n[^unused]: Nobody points here.\n";
    let d = run(md);
    assert_eq!(at(md, only(&d, Code::FootnoteUndefined)), "[^zz]");
    assert!(at(md, only(&d, Code::FootnoteUnreferenced)).starts_with("[^unused]:"));
    assert!(only(&d, Code::FootnoteNested).span.is_some());
}

#[test]
fn links_that_lose_their_target_are_reported() {
    let md = "# Title\n\n[ok](#title) [gone](#nowhere) [rel](docs/spec.md) [js](javascript:alert(1)) [web](https://example.com)\n";
    let d = run(md);
    assert_eq!(at(md, only(&d, Code::LinkAnchorMissing)), "[gone](#nowhere)");
    let dropped: Vec<_> = d.iter().filter(|d| d.code == Code::LinkDropped).collect();
    assert_eq!(dropped.len(), 2);
    assert_eq!(at(md, dropped[0]), "[rel](docs/spec.md)");
    assert_eq!(at(md, dropped[1]), "[js](javascript:alert(1))");
    // https is allowed and `#title` resolves: neither is a finding.
    assert_eq!(d.len(), 3, "{d:#?}");
}

#[test]
fn raw_html_that_loses_content_or_function_is_reported() {
    let md = "<a href=\"https://example.com\">link</a>\n\n<script>alert(1)</script>\n\ntext <iframe src=\"x\"></iframe> more\n";
    let d = run(md);
    assert!(only(&d, Code::LinkHtmlDropped).message.contains("turned off"));
    let hidden: Vec<_> = d.iter().filter(|d| d.code == Code::HtmlUnsupported).collect();
    assert_eq!(hidden.len(), 2, "{d:#?}");
    assert!(hidden.iter().any(|h| h.message.contains("<script>")));
    assert!(hidden.iter().any(|h| h.message.contains("<iframe>")));
}

#[test]
fn deep_list_nesting_is_reported_once_per_clamped_list() {
    let mut md = String::new();
    for depth in 0..12 {
        md.push_str(&format!("{}- level {depth}\n", "  ".repeat(depth)));
    }
    let d = run(&md);
    let clamped = d.iter().filter(|d| d.code == Code::ListDepthClamped).count();
    assert!(clamped >= 1, "{d:#?}");
    assert!(run("- a\n  - b\n    - c\n").is_empty());
}

#[test]
fn diagrams_and_equations_that_are_not_what_the_author_wrote_are_reported_with_original_spans() {
    // Front matter before the fence shifts every later offset; spans must still be the original's.
    let md = "---\ntitle: Spec\n---\n\n# Flow\n\n```mermaid\ngraph TD; A-->B\n```\n\nEuler $\\foo + x$ end.\n";
    let d = run(md);
    let diagram = only(&d, Code::DiagramNotRendered);
    assert_eq!(at(md, diagram), "```mermaid\ngraph TD; A-->B\n```");
    assert_eq!(diagram.span.as_ref().unwrap().line, 7);
    let math = only(&d, Code::MathUnknownCommand);
    assert_eq!(at(md, math), "$\\foo + x$");
    assert!(math.message.contains("\\foo"));
    let fm = only(&d, Code::InputFrontMatter);
    assert_eq!(fm.severity, Severity::Info);
    assert_eq!(at(md, fm), "---\ntitle: Spec\n---\n");
}

#[test]
fn a_failed_renderer_with_keep_as_code_is_a_finding_not_a_silent_success() {
    let mut cfg = Config::trusted();
    cfg.graphs.mermaid = silvercity_oss::CommandSpec::new("definitely-not-installed-tool", ["{input}", "{output}"]);
    cfg.graphs.on_failure = silvercity_oss::DiagramFailure::KeepAsCode;
    let md = "before\n\n```mermaid\ngraph TD; A-->B\n```\n\nafter\n";
    let d = convert(md, &cfg).unwrap().diagnostics;
    assert_eq!(
        at(md, only(&d, Code::DiagramKeptAsCode)),
        "```mermaid\ngraph TD; A-->B\n```"
    );
    let strict = cfg.with_loss_policy(LossPolicy::Reject);
    assert!(matches!(
        markdown_to_docx(md, &strict),
        Err(CrateError::LossyContent { .. })
    ));
}

#[test]
fn removed_control_characters_point_at_the_first_one() {
    let md = "ok\n\nbad \u{1} char and \u{2} another\n";
    let d = run(md);
    let finding = only(&d, Code::InputControlCharacters);
    assert!(finding.message.starts_with("2 character"));
    let s = finding.span.as_ref().unwrap();
    assert_eq!((s.line, s.column), (3, 5));
    assert_eq!(at(md, finding), "\u{1}");
}

#[test]
fn spans_survive_a_byte_order_mark_and_crlf_line_endings() {
    let md = "\u{feff}Line one\r\n\r\n[gone](missing.md)\r\n";
    let d = run(md);
    let finding = only(&d, Code::LinkDropped);
    assert_eq!(at(md, finding), "[gone](missing.md)");
    assert_eq!(finding.span.as_ref().unwrap().line, 3);
}

#[test]
fn strict_mode_rejects_lossy_documents_and_lists_every_finding() {
    let md = "![x](https://example.com/a.png) [y](nowhere.md)\n\n```mermaid\ngraph TD\n```\n\n$\\bogus$\n";
    let strict = Config::default().with_loss_policy(LossPolicy::Reject);
    match markdown_to_docx(md, &strict) {
        Err(CrateError::LossyContent { diagnostics }) => {
            let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
            for expected in [
                Code::ImageRemote,
                Code::LinkDropped,
                Code::DiagramNotRendered,
                Code::MathUnknownCommand,
            ] {
                assert!(codes.contains(&expected), "{expected} missing from {codes:?}");
            }
            let e = CrateError::LossyContent { diagnostics };
            assert!(e.is_input_error());
            assert!(e.to_string().contains("faithfully"), "{e}");
        }
        other => panic!("expected LossyContent, got {:?}", other.map(|d| d.len())),
    }
    // The same document under the default, permissive policy still converts.
    let c = convert(md, &Config::default()).expect("permissive");
    assert!(c.diagnostics.len() >= 4 && c.docx.starts_with(b"PK"));
}

#[test]
fn info_findings_never_trigger_rejection() {
    let md = "---\ntitle: T\n---\n\nPlain text.\n";
    let strict = Config::default().with_loss_policy(LossPolicy::Reject);
    let c = convert(md, &strict).expect("front matter alone is informational");
    assert_eq!(c.diagnostics.len(), 1);
    assert_eq!(c.diagnostics[0].severity, Severity::Info);
}

#[test]
fn analyze_reports_what_convert_reports_without_building_the_archive() {
    let md = "# T\n\n[x](gone.md) ![y](https://e.com/i.png)\n\n```mermaid\ngraph TD\n```\n";
    let cfg = Config::default();
    let analysed = analyze(md, &cfg).unwrap();
    let converted = convert(md, &cfg).unwrap();
    assert_eq!(analysed.diagnostics, converted.diagnostics);
    assert_eq!(analysed.stats, converted.stats);
    assert!(analysed.has_warnings());
    // The policy is the caller's to apply: analyze does not reject.
    let strict = cfg.with_loss_policy(LossPolicy::Reject);
    assert!(analyze(md, &strict).is_ok());
    assert!(convert(md, &strict).is_err());
}

#[test]
fn resolved_and_registered_content_is_not_a_finding() {
    let mut assets = AssetMap::new();
    assets.insert("fig", png_bytes(20, 10));
    let cfg = Config::default()
        .with_image_resolver(assets)
        .with_fence_renderer("mermaid", |_: &str| -> Result<Vec<u8>, FenceError> {
            Ok(png_bytes(20, 10))
        })
        .with_loss_policy(LossPolicy::Reject);
    let md = "![fig](fig)\n\n```mermaid\ngraph TD\n```\n";
    let c = convert(md, &cfg).expect("everything was carried over");
    assert!(c.diagnostics.is_empty(), "{:#?}", c.diagnostics);
    assert_eq!(c.stats.pictures, 2);
    let _ = Arc::new(0);
}

#[test]
fn every_code_has_a_stable_documented_identifier() {
    for code in Code::ALL {
        assert!(code.as_str().split('.').count() == 2, "{code}");
        assert!(!code.summary().is_empty());
    }
}
