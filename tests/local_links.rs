//! Links to files on disk: `LinkOptions`, the built-in rules, `LinkResolver`, and links between
//! documents converted together.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use silvercity_oss::{convert, Config, DocumentLayout, LinkDecision, LinkOptions, LinkRequest, LinkStyle, LocalLinks};

/// A source tree:
/// ```text
/// README.md  LICENSE  tools/run.exe  tests/  docs/cli.md  docs/sub/deep.md   ../outside.md
/// ```
struct Tree {
    _guard: tempfile::TempDir,
    root: PathBuf,
}

impl Tree {
    fn new() -> Tree {
        let guard = tempfile::tempdir().unwrap();
        let top = std::fs::canonicalize(guard.path()).unwrap();
        let root = top.join("project");
        for d in ["docs/sub", "tools", "tests"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let w = |p: &str, t: &str| std::fs::write(root.join(p), t).unwrap();
        w("README.md", "# Readme\n");
        w("LICENSE", "MIT\n");
        w("tools/run.exe", "MZ");
        w("tests/a.rs", "//");
        w(
            "docs/cli.md",
            "# CLI\n\n## Exit status\n\ntext\n\n## Other things\n\n<a id=\"custom\"></a>\n",
        );
        w("docs/sub/deep.md", "# Deep\n");
        std::fs::write(top.join("outside.md"), "# Outside\n").unwrap();
        Tree { _guard: guard, root }
    }

    fn cfg(&self, local: LocalLinks) -> Config {
        let mut opts = LinkOptions::default();
        opts.local = local;
        opts.source_dir = Some(self.root.clone());
        Config::native_math().with_link_options(opts)
    }
}

fn rels(docx: &[u8]) -> String {
    zip_text(docx, "word/_rels/document.xml.rels")
}

fn codes(c: &silvercity_oss::Conversion) -> Vec<&str> {
    c.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn options(cfg: &mut Config) -> &mut LinkOptions {
    &mut cfg.links
}

#[test]
fn relative_paths_stay_text_by_default_in_every_preset() {
    let t = Tree::new();
    for cfg in [Config::default(), Config::native_math()] {
        let mut opts = LinkOptions::default();
        opts.source_dir = Some(t.root.clone());
        let c = convert("[x](docs/cli.md)", &cfg.with_link_options(opts)).unwrap();
        assert!(codes(&c).contains(&"link.dropped"), "{:?}", codes(&c));
        assert!(!rels(&c.docx).contains("cli.md"));
    }
}

#[test]
fn local_links_are_rebased_to_where_the_document_is_saved() {
    let t = Tree::new();
    let mut cfg = t.cfg(LocalLinks::Local);
    options(&mut cfg).output_dir = Some(t.root.join("build/out"));
    let md = "[a](docs/cli.md) [b](docs/sub/deep.md) [c](LICENSE) [d](tests/) [e](/docs/cli.md)";
    let c = convert(md, &cfg).unwrap();
    assert!(codes(&c).is_empty(), "{:?}", c.diagnostics);
    let r = rels(&c.docx);
    for want in [
        "Target=\"../../docs/cli.md\"",
        "Target=\"../../docs/sub/deep.md\"",
        "Target=\"../../LICENSE\"",
        "Target=\"../../tests/\"",
    ] {
        assert!(r.contains(want), "{want} not in {r}");
    }
    // `/docs/cli.md` is the same file as `docs/cli.md`.
    assert_eq!(r.matches("../../docs/cli.md").count(), 2, "{r}");
    assert_eq!(c.stats.hyperlinks, 5);
}

#[test]
fn the_output_next_to_the_source_keeps_paths_as_written() {
    let t = Tree::new();
    let c = convert("[a](docs/cli.md) [b](./docs/../LICENSE)", &t.cfg(LocalLinks::Local)).unwrap();
    let r = rels(&c.docx);
    assert!(
        r.contains("Target=\"docs/cli.md\"") && r.contains("Target=\"LICENSE\""),
        "{r}"
    );
}

#[test]
fn absolute_style_writes_file_addresses() {
    let t = Tree::new();
    let mut cfg = t.cfg(LocalLinks::Local);
    options(&mut cfg).style = LinkStyle::Absolute;
    let c = convert("[a](docs/cli.md) [d](tests/)", &cfg).unwrap();
    let r = rels(&c.docx);
    let base = format!("file://{}", t.root.display());
    assert!(r.contains(&format!("Target=\"{base}/docs/cli.md\"")), "{r}");
    assert!(r.contains(&format!("Target=\"{base}/tests/\"")), "{r}");
}

#[test]
fn local_mode_refuses_what_it_cannot_vouch_for_and_says_why() {
    let t = Tree::new();
    let cfg = t.cfg(LocalLinks::Local);
    let md = "[gone](docs/nope.md) [out](../outside.md) [exe](tools/run.exe) [net](//host/share/x) [unc](\\\\host\\share\\x) [abs](C:/Windows/x) [pct](docs/%00x)";
    let c = convert(md, &cfg).unwrap();
    let got = codes(&c);
    for want in ["link.target-missing", "link.outside-root", "link.blocked"] {
        assert!(got.contains(&want), "{want} in {got:?}");
    }
    assert_eq!(c.stats.hyperlinks, 0);
    assert_eq!(rels(&c.docx).matches("Hyperlink").count(), 0);
    // The text is still there.
    assert!(visible_text(&zip_text(&c.docx, "word/document.xml")).contains("gone"));
}

#[test]
fn all_mode_links_missing_outside_and_program_targets_but_never_network_paths() {
    let t = Tree::new();
    let mut cfg = t.cfg(LocalLinks::All);
    options(&mut cfg).style = LinkStyle::Absolute;
    let c = convert(
        "[gone](docs/nope.md) [out](../outside.md) [exe](tools/run.exe) [net](//host/share/x)",
        &cfg,
    )
    .unwrap();
    let r = rels(&c.docx);
    assert!(
        r.contains("docs/nope.md") && r.contains("outside.md") && r.contains("run.exe"),
        "{r}"
    );
    assert!(!r.contains("host/share"), "{r}");
    let got = codes(&c);
    assert!(
        got.contains(&"link.target-missing") && got.contains(&"link.blocked"),
        "{got:?}"
    );
    assert_eq!(c.stats.hyperlinks, 3);
}

#[test]
fn file_addresses_work_in_all_mode_only() {
    let t = Tree::new();
    let md = format!("[l](file://{}/LICENSE)", t.root.display());
    let all = convert(&md, &t.cfg(LocalLinks::All)).unwrap();
    assert!(rels(&all.docx).contains("LICENSE"));
    let local = convert(&md, &t.cfg(LocalLinks::Local)).unwrap();
    assert!(!rels(&local.docx).contains("LICENSE"));
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_root_is_outside_the_root() {
    let t = Tree::new();
    std::os::unix::fs::symlink(
        t.root.parent().unwrap().join("outside.md"),
        t.root.join("docs/sneaky.md"),
    )
    .unwrap();
    let c = convert("[s](docs/sneaky.md)", &t.cfg(LocalLinks::Local)).unwrap();
    assert!(codes(&c).contains(&"link.outside-root"), "{:?}", codes(&c));
    assert_eq!(c.stats.hyperlinks, 0);
}

#[test]
fn headings_in_linked_markdown_are_checked() {
    let t = Tree::new();
    let cfg = t.cfg(LocalLinks::Local);
    let ok = convert(
        "[a](docs/cli.md#exit-status) [b](docs/cli.md#custom) [c](docs/cli.md#Other-Things)",
        &cfg,
    )
    .unwrap();
    assert!(codes(&ok).is_empty(), "{:?}", ok.diagnostics);
    let bad = convert("[a](docs/cli.md#no-such-heading)", &cfg).unwrap();
    assert_eq!(codes(&bad), ["link.anchor-missing"], "{:?}", bad.diagnostics);
    assert_eq!(bad.stats.hyperlinks, 1, "the file link is still made");
    let mut off = t.cfg(LocalLinks::Local);
    options(&mut off).check_anchors = false;
    assert!(codes(&convert("[a](docs/cli.md#no-such-heading)", &off).unwrap()).is_empty());
}

#[test]
fn the_report_lists_what_was_linked() {
    let t = Tree::new();
    let c = convert(
        "[a](docs/cli.md) [b](docs/cli.md#exit-status) [c](tests/) [d](LICENSE) [e](docs/nope.md)",
        &t.cfg(LocalLinks::Local),
    )
    .unwrap();
    assert_eq!(c.local_links.len(), 3, "{:?}", c.local_links);
    let cli = c.local_links.iter().find(|l| l.path.ends_with("docs/cli.md")).unwrap();
    assert!(cli.markdown && !cli.directory);
    assert!(c.local_links.iter().any(|l| l.directory && l.path.ends_with("tests")));
    assert!(c.local_links.iter().any(|l| !l.markdown && l.path.ends_with("LICENSE")));
}

#[test]
fn local_files_can_become_web_addresses() {
    let t = Tree::new();
    let mut cfg = t.cfg(LocalLinks::Local);
    {
        let o = options(&mut cfg);
        o.base_url = Some("https://example.com/me/repo/blob/v1/".into());
        o.dir_base_url = Some("https://example.com/me/repo/tree/v1/".into());
        o.rewrites
            .push(("docs/sub/".into(), "https://docs.example.com/deep/".into()));
    }
    let c = convert(
        "[a](docs/cli.md#exit-status) [b](tests/) [c](docs/sub/deep.md) [d](LICENSE)",
        &cfg,
    )
    .unwrap();
    assert!(codes(&c).is_empty(), "{:?}", c.diagnostics);
    let r = rels(&c.docx);
    for want in [
        "https://example.com/me/repo/blob/v1/docs/cli.md#exit-status",
        "https://example.com/me/repo/tree/v1/tests/",
        "https://docs.example.com/deep/deep.md",
        "https://example.com/me/repo/blob/v1/LICENSE",
    ] {
        assert!(r.contains(want), "{want} not in {r}");
    }
}

#[test]
fn rewrites_and_documents_need_local_links_on() {
    let mut cfg = Config::default();
    cfg.links.base_url = Some("https://example.com/".into());
    assert!(convert("x", &cfg).is_err());
    let mut cfg = Config::default();
    cfg.links.documents = Some(DocumentLayout::mirrored("/a", "/b"));
    assert!(convert("x", &cfg).is_err());
}

// ---------------------------------------------------------------- documents

fn layout(t: &Tree, out: &Path) -> LinkOptions {
    let mut o = LinkOptions::default();
    o.local = LocalLinks::Local;
    o.source_dir = Some(t.root.clone());
    o.documents = Some(DocumentLayout::mirrored(&t.root, out));
    o
}

#[test]
fn links_to_markdown_files_point_at_the_documents_they_become() {
    let t = Tree::new();
    let out = t.root.parent().unwrap().join("out");
    let mut o = layout(&t, &out);
    o.output_dir = Some(out.clone());
    let cfg = Config::native_math().with_link_options(o);
    let c = convert("[a](docs/cli.md#exit-status) [b](docs/sub/deep.md) [c](LICENSE)", &cfg).unwrap();
    assert!(codes(&c).is_empty(), "{:?}", c.diagnostics);
    let doc = zip_text(&c.docx, "word/document.xml");
    let r = rels(&c.docx);
    assert!(r.contains("Target=\"docs/cli.docx\""), "{r}");
    assert!(r.contains("Target=\"docs/sub/deep.docx\""), "{r}");
    // The other document's heading is found by a bookmark named from its anchor.
    let bookmark = {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(b"exit-status");
        format!("md_s{}", d[..8].iter().map(|b| format!("{b:02x}")).collect::<String>())
    };
    assert!(doc.contains(&format!("w:anchor=\"{bookmark}\"")), "{doc}");
    // Not a Markdown file and nothing copied: it stays where it is, relative to the output.
    assert!(r.contains("Target=\"../project/LICENSE\""), "{r}");
}

#[test]
fn a_converted_document_exports_a_bookmark_for_each_heading_and_anchor() {
    let t = Tree::new();
    let out = t.root.parent().unwrap().join("out");
    let mut o = layout(&t, &out);
    o.output_dir = Some(out.join("docs"));
    o.source_dir = Some(t.root.join("docs"));
    let cfg = Config::native_math().with_link_options(o);
    let md = std::fs::read_to_string(t.root.join("docs/cli.md")).unwrap();
    let c = convert(&md, &cfg).unwrap();
    let doc = zip_text(&c.docx, "word/document.xml");
    for name in ["cli", "exit-status", "other-things", "custom"] {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(name.as_bytes());
        let bookmark = format!("md_s{}", d[..8].iter().map(|b| format!("{b:02x}")).collect::<String>());
        assert!(doc.contains(&format!("w:name=\"{bookmark}\"")), "{name}: {doc}");
    }
    assert_well_formed(&doc);
    // And a document that is not part of a set writes none.
    let plain = convert(&md, &Config::native_math()).unwrap();
    assert!(!zip_text(&plain.docx, "word/document.xml").contains("md_s"));
}

#[test]
fn links_climb_back_up_from_a_nested_document() {
    let t = Tree::new();
    let out = t.root.parent().unwrap().join("out");
    let mut o = layout(&t, &out);
    o.source_dir = Some(t.root.join("docs"));
    o.output_dir = Some(out.join("docs"));
    let cfg = Config::native_math().with_link_options(o);
    let c = convert("[home](../README.md) [sib](cli.md) [deep](sub/deep.md)", &cfg).unwrap();
    assert!(codes(&c).is_empty(), "{:?}", c.diagnostics);
    let r = rels(&c.docx);
    for want in ["../README.docx", "Target=\"cli.docx\"", "sub/deep.docx"] {
        assert!(r.contains(want), "{want} in {r}");
    }
}

#[test]
fn a_flat_layout_puts_every_document_in_one_directory() {
    let t = Tree::new();
    let out = t.root.parent().unwrap().join("out");
    let mut o = layout(&t, &out);
    o.output_dir = Some(out.clone());
    if let Some(d) = o.documents.as_mut() {
        d.mirror = false;
    }
    let c = convert("[deep](docs/sub/deep.md)", &Config::native_math().with_link_options(o)).unwrap();
    assert!(rels(&c.docx).contains("Target=\"deep.docx\""), "{}", rels(&c.docx));
}

#[test]
fn copied_files_are_linked_where_the_copy_will_be() {
    let t = Tree::new();
    let out = t.root.parent().unwrap().join("out");
    let mut o = layout(&t, &out);
    o.output_dir = Some(out.clone());
    if let Some(d) = o.documents.as_mut() {
        d.copy_files = true;
    }
    let c = convert("[l](LICENSE) [t](tests/)", &Config::native_math().with_link_options(o)).unwrap();
    let r = rels(&c.docx);
    assert!(
        r.contains("Target=\"LICENSE\"") && r.contains("Target=\"tests/\""),
        "{r}"
    );
}

// ---------------------------------------------------------------- resolver

#[test]
fn a_resolver_decides_links_before_the_built_in_rules() {
    let resolver = |req: &LinkRequest<'_>| -> LinkDecision {
        match req.target {
            "wiki:Home" => LinkDecision::Url("https://wiki.example.com/Home".into()),
            "other" => LinkDecision::Document {
                target: "other.docx".into(),
                heading: Some("Intro".into()),
            },
            "toc" => LinkDecision::Heading("title".into()),
            "hidden" => LinkDecision::Text,
            "bad" => LinkDecision::Url("https://x/\u{1}".into()),
            _ => LinkDecision::Default,
        }
    };
    let cfg = Config::default().with_link_resolver(resolver);
    let md = "# Title\n\n[w](wiki:Home) [o](other) [t](toc) [h](hidden) [b](bad) [n](https://example.com/)\n";
    let c = convert(md, &cfg).unwrap();
    let r = rels(&c.docx);
    let doc = zip_text(&c.docx, "word/document.xml");
    assert!(
        r.contains("https://wiki.example.com/Home") && r.contains("other.docx") && r.contains("https://example.com/")
    );
    assert!(doc.contains("w:anchor=\"md_s"), "the other document's heading");
    assert!(doc.contains("w:anchor=\"md_h0\""), "this document's heading");
    assert_eq!(c.stats.hyperlinks, 4, "{:?}", c.diagnostics);
    assert!(codes(&c).contains(&"link.dropped"), "the unusable address is reported");
    let text = visible_text(&doc);
    assert!(text.contains('h') && text.contains('b'));
}

#[test]
fn a_panicking_resolver_costs_the_link_not_the_process() {
    let cfg = Config::default().with_link_resolver(|_: &LinkRequest<'_>| -> LinkDecision { panic!("boom") });
    let c = convert("[x](https://example.com/)", &cfg).unwrap();
    assert_eq!(c.stats.hyperlinks, 0);
    assert!(codes(&c).contains(&"link.dropped"));
}
