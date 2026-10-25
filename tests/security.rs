//! Security gates: hostile Markdown must end in a clean error or a harmless
//! document, never a crash, a hang, a leaked file, or a corrupt `.docx`.

mod common;

use common::*;
use silvercity_oss::__internals::preprocess;
use silvercity_oss::{markdown_to_docx, Config, CrateError, ImageAccess, Limits};

fn err(md: &str, cfg: &Config) -> CrateError {
    markdown_to_docx(md, cfg).expect_err("should have been rejected")
}

// ------------------------------------------------------------------ crashes

#[test]
fn deeply_nested_formulas_are_errors_not_stack_overflows() {
    let cfg = Config::native_math();
    // Each stays under the 16 KiB formula limit, so the nesting guard itself is exercised.
    let shapes = [
        format!("$${}x{}$$", "{".repeat(4000), "}".repeat(4000)),
        format!("$$x{}1{}$$", "^{".repeat(3000), "}".repeat(3000)),
        format!("$${}x$$", "\\mathbf".repeat(2000)),
        format!("$${}x$$", "\\frac".repeat(3000)),
        format!("$${}x{}$$", "\\left(".repeat(1000), "\\right)".repeat(1000)),
        format!("$${}a{}$$", "\\begin{matrix}".repeat(600), "\\end{matrix}".repeat(600)),
        format!("$${}x{}$$", "\\mathrm{".repeat(1500), "}".repeat(1500)),
    ];
    for (i, md) in shapes.iter().enumerate() {
        match markdown_to_docx(md, &cfg) {
            Err(CrateError::InvalidMarkdown(m)) => assert!(
                m.contains("nested") || m.contains("unclosed") || m.contains("missing"),
                "shape {i}: {m}"
            ),
            Err(other) => panic!("shape {i}: unexpected error {other}"),
            Ok(_) => panic!("shape {i} should be rejected"),
        }
    }
}

#[test]
fn deeply_nested_markdown_structure_does_not_crash() {
    let cfg = Config::untrusted();
    for md in [
        format!("{} deep\n", ">".repeat(20_000)),
        (0..1500)
            .map(|i| format!("{}- item\n", "  ".repeat(i % 400)))
            .collect::<String>(),
        format!("{}x{}\n", "*".repeat(50_000), "*".repeat(50_000)),
        format!("{}\n", "[".repeat(100_000)),
        format!("{}\n", "![a](".repeat(20_000)),
        format!("{}\n", "`".repeat(100_000)),
        (0..3000).map(|i| format!("| {i} | b |\n")).collect::<String>(),
    ] {
        // Success or a clean error are both fine; reaching this line means no crash or hang.
        let _ = markdown_to_docx(&md, &cfg);
    }
}

// ------------------------------------------------------------------- limits

#[test]
fn every_limit_is_enforced() {
    let small = |f: fn(&mut Limits)| {
        let mut c = Config::native_math();
        f(&mut c.limits);
        c
    };
    assert!(matches!(
        err("x".repeat(100).as_str(), &small(|l| l.max_input_bytes = 50)),
        CrateError::LimitExceeded(m) if m.contains("input")
    ));
    assert!(matches!(
        err("$a$ $b$ $c$", &small(|l| l.max_rendered_blocks = 2)),
        CrateError::LimitExceeded(m) if m.contains("formulas")
    ));
    assert!(matches!(
        err("$$\\frac{aaaaaaaaaa}{b}$$", &small(|l| l.max_formula_bytes = 8)),
        CrateError::LimitExceeded(m) if m.contains("formula")
    ));
    let mut with_graphs = Config::trusted();
    with_graphs.limits.max_graph_source_bytes = 10;
    assert!(matches!(
        err("```mermaid\ngraph TD; A-->B-->C-->D\n```\n", &with_graphs),
        CrateError::LimitExceeded(m) if m.contains("diagram")
    ));
}

#[test]
fn image_size_limits_apply_per_file_and_in_total() {
    let fx = Fixture::new();
    let size = std::fs::metadata(fx.path("fixture.png")).unwrap().len();
    let mut cfg = Config::native_math();
    cfg.base_dir = Some(fx.dir.path().to_path_buf());

    cfg.limits.max_image_bytes = size - 1;
    assert!(matches!(err("![x](fixture.png)", &cfg), CrateError::LimitExceeded(_)));

    cfg.limits.max_image_bytes = size;
    cfg.limits.max_total_image_bytes = size * 2;
    assert!(markdown_to_docx("![a](fixture.png) ![b](fixture.png)", &cfg).is_ok());
    assert!(matches!(
        err("![a](fixture.png) ![b](fixture.png) ![c](fixture.png)", &cfg),
        CrateError::LimitExceeded(m) if m.contains("in total")
    ));
}

#[test]
fn footnote_references_cannot_amplify_the_document() {
    let mut cfg = Config::native_math();
    cfg.limits.max_input_bytes = 200_000;
    // ~100 KB definition referenced 50 times would expand to ~5 MB.
    let md = format!("{}\n\n[^a]: {}\n", "Ref[^a] ".repeat(50), "word ".repeat(20_000));
    assert!(matches!(err(&md, &cfg), CrateError::LimitExceeded(m) if m.contains("footnotes")));
}

// ---------------------------------------------------------- XML integrity

#[test]
fn control_characters_never_reach_the_xml() {
    let md = "plain \u{1} a\u{b}b \u{1f} \u{FFFE} \u{FFFF}\n\n\
              # Head\u{2}ing\n\n`co\u{3}de`\n\n```\nfen\u{4}ce\n```\n\n\
              [li\u{5}nk](https://example.com/\u{6}x)\n\n![al\u{7}t](missing\u{8}.png)\n\n\
              <b>ht\u{9}ml &#1;&#xB; &#0;</b>\n\nfoot[^1]\n\n[^1]: note\u{e}\n\n| a\u{f} | b |\n|---|---|\n| c | d\u{10} |\n";
    let mut cfg = Config::native_math();
    cfg.image_access = ImageAccess::None;
    let docx = markdown_to_docx(md, &cfg).unwrap();
    for part in [
        "word/document.xml",
        "word/footnotes.xml",
        "word/_rels/document.xml.rels",
    ] {
        if zip_entries(&docx).iter().any(|n| n == part) {
            let xml = zip_text(&docx, part);
            assert_only_xml_chars(&xml);
            assert_well_formed(&xml);
        }
    }
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(text.contains("plain") && text.contains("Heading") && text.contains("code") && text.contains("fence"));
}

#[test]
fn text_with_markup_characters_is_escaped_not_interpreted() {
    let md = "a <w:p>fake</w:p> & \"quotes\" 'x' ]]> <m:oMath>no</m:oMath>\n\n`</w:t></w:r>`\n\n\
              [t](https://e.com/?a=1&b=\"2\"<3>) \n";
    let b = body(md);
    assert!(!b.contains("<w:p>fake"), "{b}");
    assert!(!b.contains("<m:oMath>no"));
    assert!(
        visible_text(&b).contains("a <w:p>fake</w:p> & \"quotes\""),
        "markup-looking text must survive as text"
    );
}

#[test]
fn a_hostile_document_cannot_forge_equation_placeholders() {
    // Placeholders are derived from the document's own hash, so text can't name one.
    let md = "SILVERCITYOMML0000000000000000X0X and $x$ and SILVERCITYOMMLffffffffffffffffX0X\n";
    let b = body(md);
    assert!(b.contains("<m:oMath>"));
    assert!(
        visible_text(&b).contains("SILVERCITYOMML0000000000000000X0X"),
        "literal text must survive"
    );
}

// -------------------------------------------------------------------- links

#[test]
fn only_allowlisted_link_schemes_become_hyperlinks() {
    let md = "[ok](https://ok.example/a) [mail](mailto:a@b.example) [js](javascript:alert(1)) \
              [file](file:///etc/passwd) [unc](\\\\\\\\evil\\\\share\\\\x) [app](ms-excel:ofe|u|http://x) \
              [data](data:text/html;base64,AAAA) [anchor](#top) [rel](../x.md) [tab](java\tscript:1) \
              <javascript:alert(2)> <https://auto.example/>\n";
    let docx = markdown_to_docx(md, &Config::native_math()).unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    let targets: Vec<&str> = rels
        .split("Target=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .collect();
    for t in &targets {
        let l = t.to_ascii_lowercase();
        for bad in [
            "javascript",
            "file:",
            "evil",
            "ms-excel",
            "data:",
            "#top",
            "../x.md",
            "script",
        ] {
            assert!(!l.contains(bad), "dangerous target survived: {t}");
        }
    }
    for good in ["https://ok.example/a", "mailto:a@b.example", "https://auto.example/"] {
        assert!(targets.contains(&good), "{good} missing from {targets:?}");
    }
    // The visible text of refused links stays readable.
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    for label in ["js", "file", "unc", "app", "data", "anchor", "rel"] {
        assert!(text.contains(label), "link text `{label}` was lost");
    }
}

#[test]
fn link_schemes_are_configurable() {
    let mut cfg = Config::native_math();
    cfg.allowed_link_schemes = vec!["https".into()];
    let docx = markdown_to_docx(
        "[m](mailto:a@b.example) [h](http://plain.example) [s](https://s.example)",
        &cfg,
    )
    .unwrap();
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(rels.contains("https://s.example") && !rels.contains("mailto:") && !rels.contains("http://plain"));
}

// ------------------------------------------------------------------- images

#[test]
fn images_cannot_escape_the_base_directory() {
    let fx = Fixture::new();
    let base = fx.path("docs");
    std::fs::create_dir_all(base.join("img")).unwrap();
    std::fs::copy(fx.path("fixture.png"), base.join("img/ok.png")).unwrap();
    std::fs::copy(fx.path("fixture.png"), fx.path("secret.png")).unwrap();
    let cfg = Config::native_math().with_base_dir(base.clone());

    assert_eq!(media_count(&markdown_to_docx("![ok](img/ok.png)", &cfg).unwrap()), 1);
    for evil in [
        "![x](../secret.png)",
        "![x](img/../../secret.png)",
        &format!("![x]({})", fx.path("secret.png").display()),
        &format!("![x](file://{})", fx.path("secret.png").display()),
        "![x](%2e%2e/secret.png)",
        "![x](/etc/passwd)",
    ] {
        match markdown_to_docx(evil, &cfg) {
            Err(CrateError::UnsafeContent(_)) | Err(CrateError::Io { .. }) => {}
            other => panic!("{evil} should be refused, got {:?}", other.map(|d| d.len())),
        }
    }
}

#[cfg(unix)]
#[test]
fn symlinks_and_device_files_cannot_be_embedded() {
    let fx = Fixture::new();
    let base = fx.path("docs");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::copy(fx.path("fixture.png"), fx.path("secret.png")).unwrap();
    std::os::unix::fs::symlink(fx.path("secret.png"), base.join("link.png")).unwrap();
    let mut cfg = Config::native_math().with_base_dir(base);
    assert!(matches!(err("![x](link.png)", &cfg), CrateError::UnsafeContent(_)));
    // Even a fully trusting configuration refuses endless or blocking files.
    cfg.image_access = ImageAccess::Any;
    for special in ["/dev/zero", "/dev/urandom", "/dev/null"] {
        assert!(
            matches!(err(&format!("![x]({special})"), &cfg), CrateError::UnsafeContent(_)),
            "{special}"
        );
    }
}

#[test]
fn image_access_none_and_unsupported_formats_degrade_to_alt_text() {
    let fx = Fixture::new();
    std::fs::write(fx.path("pic.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    std::fs::write(fx.path("broken.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0, 1, 2]).unwrap();
    let mut cfg = Config::native_math();
    cfg.base_dir = Some(fx.dir.path().to_path_buf());

    let docx = markdown_to_docx("![vector](pic.svg) ![corrupt](broken.jpg)", &cfg).unwrap();
    assert_eq!(media_count(&docx), 0);
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(text.contains("[vector]") && text.contains("[corrupt]"), "{text}");

    cfg.image_access = ImageAccess::None;
    let docx = markdown_to_docx(
        "![logo](fixture.png) ![r](https://example.com/x.png) ![d](data:image/png;base64,AAAA)",
        &cfg,
    )
    .unwrap();
    assert_eq!(media_count(&docx), 0);
    let text = visible_text(&zip_text(&docx, "word/document.xml"));
    assert!(text.contains("[logo]") && text.contains("[r]") && text.contains("[d]"));
}

#[test]
fn huge_claimed_image_dimensions_are_rejected_cheaply() {
    let fx = Fixture::new();
    let cfg = Config::native_math().with_base_dir(fx.dir.path());
    // Header says 60000 x 60000 (3.6 billion pixels); the file is a few dozen bytes.
    std::fs::write(fx.path("bomb.png"), png_claiming(60_000, 60_000)).unwrap();
    assert!(matches!(err("![x](bomb.png)", &cfg), CrateError::LimitExceeded(m) if m.contains("pixels")));
    // Absurd dimensions of either axis alone.
    for (w, h) in [(u32::MAX, 1), (1, u32::MAX), (u32::MAX, u32::MAX)] {
        std::fs::write(fx.path("bomb2.png"), png_claiming(w, h)).unwrap();
        assert!(
            !matches!(markdown_to_docx("![x](bomb2.png)", &cfg), Ok(d) if media_count(&d) > 0),
            "{w}x{h}"
        );
    }
    // A header-only PNG with no data chunks is "not an image": alt text, no crash.
    let mut headless = png_claiming(10, 10);
    headless.truncate(8 + 25);
    std::fs::write(fx.path("headless.png"), headless).unwrap();
    assert_eq!(media_count(&markdown_to_docx("![x](headless.png)", &cfg).unwrap()), 0);
}

// ---------------------------------------------------------------- the TeX gate

#[test]
fn tex_that_touches_the_host_is_refused_before_latex_runs() {
    // `latex` points at a program that does not exist: reaching it would be MissingExternalCLI.
    let mut cfg = Config::trusted();
    cfg.math.engine = silvercity_oss::MathImageEngine::LatexDvipng {
        latex: "latex-that-must-never-run".into(),
        dvipng: "dvipng-that-must-never-run".into(),
    };
    for evil in [
        r"$\input{/etc/passwd}$",
        r"$$\include{x}$$",
        r"$\openin5=/etc/passwd \read5 to\x$",
        r"$\immediate\write18{id}$",
        r"$\catcode`\^^M=12$",
        r"$\expandafter\csname in\string put\endcsname{/etc/passwd}$",
        r"$\^^69nput{/etc/passwd}$",
        r"$\usepackage{x}$",
        r"$\special{psfile=/etc/passwd}$",
        r"$\pdffiledump offset 0 length 1{/etc/passwd}$",
    ] {
        match markdown_to_docx(evil, &cfg) {
            Err(CrateError::UnsafeContent(_)) => {}
            other => panic!("{evil}: expected UnsafeContent, got {:?}", other.map(|d| d.len())),
        }
    }
    // Ordinary math gets past the gate (and then fails only because latex is absent).
    assert!(matches!(
        markdown_to_docx(r"$\frac{a}{b}$", &cfg),
        Err(CrateError::MissingExternalCLI { .. })
    ));
}

// ------------------------------------------------------------ untrusted preset

#[test]
fn untrusted_preset_never_starts_a_process_or_reads_a_file() {
    let fx = Fixture::new();
    let witness = fx.path("witness");
    let mut cfg = Config::untrusted();
    cfg.base_dir = Some(fx.dir.path().to_path_buf());
    // If anything tried to run these, `MissingExternalCLI` or the witness would show it.
    cfg.graphs.mermaid.program = "must-not-run-mmdc".into();
    cfg.graphs.dolphin.program = "must-not-run-dolphin".into();

    let md = "```mermaid\ngraph TD; A-->B\n```\n\n```dolphin\nnode\n```\n\n$$\\frac{1}{2}$$\n\nand $x^2$\n\n\
              ![local](fixture.png)\n\n<script>alert(1)</script><img src=\"fixture.png\" alt=\"html img\">\n";
    let docx = markdown_to_docx(md, &cfg).unwrap();
    let b = zip_text(&docx, "word/document.xml");
    assert_well_formed(&b);
    assert_eq!(media_count(&docx), 0, "no local image embedded");
    assert!(
        b.contains("<m:oMathPara>") && b.contains("<m:oMath>"),
        "native math still works"
    );
    let text = visible_text(&b);
    assert!(
        text.contains("graph TD; A--&gt;B") || text.contains("graph TD; A-->B"),
        "diagram stays as code: {text}"
    );
    assert!(text.contains("[local]") && text.contains("[html img]"));
    assert!(!text.contains("alert"), "script content is dropped");
    assert!(!witness.exists());
}

#[test]
fn preprocess_with_diagrams_disabled_leaves_fences_alone() {
    let mut cfg = Config::native_math();
    cfg.graphs.enabled = false;
    let md = "```mermaid\ngraph TD; A-->B\n```\n";
    let pre = preprocess(md, &cfg).unwrap();
    assert_eq!(pre.markdown, md);
    assert!(pre.assets.is_empty());
}

// ------------------------------------------------------------- randomised

/// Run `f` on another thread and fail (instead of hanging CI) if it takes longer than `secs`.
fn within<F: FnOnce() + Send + 'static>(secs: u64, f: F) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs))
        .unwrap_or_else(|_| panic!("did not finish within {secs}s: a hang or runaway loop"));
}

/// xorshift64*; deterministic so failures reproduce.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

const MD_FRAGMENTS: &[&str] = &[
    "# ",
    "## ",
    "> ",
    "- ",
    "1. ",
    "* ",
    "    ",
    "\t",
    "```",
    "~~~",
    "```mermaid\n",
    "$",
    "$$",
    "\\",
    "\n",
    "\n\n",
    "[",
    "](",
    ")",
    "![",
    "<",
    ">",
    "<b>",
    "</b>",
    "<div>",
    "<!--",
    "-->",
    "<script>",
    "&amp;",
    "&#1;",
    "&#x110000;",
    "|",
    "---",
    "***",
    "___",
    "+++",
    "[^1]",
    "[^1]: ",
    ": ",
    "[!NOTE]",
    "{#id}",
    "`",
    "``",
    "**",
    "__",
    "~~",
    "*",
    "_",
    "text",
    "word ",
    "日本語",
    "😀",
    "\u{1}",
    "\u{202e}",
    "\r\n",
    "\0",
    "http://",
    "javascript:",
    "file:///x",
    "\\frac{",
    "}",
    "{",
    "^",
    "_{",
    "\\left(",
    "\\right)",
    "\\begin{matrix}",
    "\\end{matrix}",
    "&",
    "\\\\",
    "\\sum_",
    "\\input",
    "\\text{",
    "x",
    "1",
    " ",
];

#[test]
fn randomised_markdown_never_panics_or_corrupts_output() {
    within(120, randomised_markdown_body);
}

fn randomised_markdown_body() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let cfg = Config::untrusted();
    let mut ok = 0;
    for _ in 0..1500 {
        let n = 1 + (rng.next() % 60) as usize;
        let md: String = (0..n).map(|_| rng.pick(MD_FRAGMENTS)).collect();
        // A clean rejection is fine; a panic, hang, or corrupt document is not.
        if let Ok(docx) = markdown_to_docx(&md, &cfg) {
            ok += 1;
            let xml = zip_text(&docx, "word/document.xml");
            assert_only_xml_chars(&xml);
            assert_well_formed(&xml);
        }
    }
    assert!(
        ok > 500,
        "generator produced too few valid documents ({ok}); the test would prove little"
    );
}

#[test]
fn randomised_latex_never_panics() {
    within(60, randomised_latex_body);
}

fn randomised_latex_body() {
    let mut rng = Rng(0xD1B54A32D192ED03);
    let pieces = [
        "\\frac",
        "{",
        "}",
        "^",
        "_",
        "\\sqrt",
        "[",
        "]",
        "\\left",
        "\\right",
        "(",
        ")",
        "|",
        "\\begin{pmatrix}",
        "\\end{pmatrix}",
        "\\begin{cases}",
        "\\end{cases}",
        "&",
        "\\\\",
        "\\sum",
        "\\int",
        "\\text",
        "\\mathbf",
        "\\hat",
        "\\overline",
        "'",
        "x",
        "1",
        "+",
        "-",
        "=",
        " ",
        "\\alpha",
        "\\",
        "%",
        "$",
        "~",
        "é",
        "日",
    ];
    for _ in 0..5000 {
        let n = 1 + (rng.next() % 40) as usize;
        let latex: String = (0..n).map(|_| rng.pick(&pieces)).collect();
        let _ = silvercity_oss::omml::latex_to_omml(&latex);
    }
}
