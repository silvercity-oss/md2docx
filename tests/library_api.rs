//! The public library API as a downstream crate sees it: configuration,
//! validation, writers, files, and concurrent use.

mod common;

use std::io::Cursor;

use common::*;
use silvercity_oss::{
    markdown_file_to_docx, markdown_to_docx, markdown_to_docx_writer, Config, CrateError, DocumentOptions, ImageAccess,
    Limits, MermaidEngine, PageSize,
};

fn with_document(f: impl FnOnce(&mut DocumentOptions)) -> Config {
    let mut d = DocumentOptions::default();
    f(&mut d);
    Config::native_math().with_document(d)
}

fn page_attrs(docx: &[u8]) -> (String, String) {
    let b = zip_text(docx, "word/document.xml");
    let at = |tag: &str| {
        let i = b.find(tag).unwrap_or_else(|| panic!("{tag} missing in {b}"));
        b[i..].split("/>").next().unwrap().to_string()
    };
    (at("<w:pgSz"), at("<w:pgMar"))
}

// ------------------------------------------------------------ page and fonts

#[test]
fn default_page_is_a4_with_one_inch_margins() {
    let docx = markdown_to_docx("hi", &Config::native_math()).unwrap();
    let (size, margin) = page_attrs(&docx);
    assert!(
        size.contains("w:w=\"11906\"") && size.contains("w:h=\"16838\""),
        "{size}"
    );
    assert!(
        margin.contains("w:left=\"1440\"") && margin.contains("w:top=\"1440\""),
        "{margin}"
    );
}

#[test]
fn page_size_margins_and_fonts_are_configurable() {
    let cfg = with_document(|d| {
        d.page = PageSize::Letter;
        d.margin_twips = 720;
        d.body_font = "Georgia".into();
        d.body_size_half_points = 24;
        d.code_font = "Courier New".into();
        d.heading_color = "C00000".into();
    });
    let docx = markdown_to_docx("# Title\n\nbody\n\n```\ncode\n```\n", &cfg).unwrap();
    let (size, margin) = page_attrs(&docx);
    assert!(
        size.contains("w:w=\"12240\"") && size.contains("w:h=\"15840\""),
        "{size}"
    );
    for side in ["top", "bottom", "left", "right"] {
        assert!(margin.contains(&format!("w:{side}=\"720\"")), "{side}: {margin}");
    }
    let styles = zip_text(&docx, "word/styles.xml");
    assert!(
        styles.contains("Georgia") && styles.contains("Courier New"),
        "fonts missing"
    );
    assert!(
        !styles.contains("Calibri") && !styles.contains("Consolas"),
        "defaults must be replaced"
    );
    assert!(styles.contains("C00000") && !styles.contains("1F3864"));
    assert!(styles.contains("w:val=\"24\""), "body size 12pt");
}

#[test]
fn landscape_and_custom_sizes_work_and_scale_image_limits() {
    let fx = Fixture::new(); // fixture.png: 600 x 300 px
    let md = "![wide](fixture.png)";
    let cx = |cfg: &Config| -> u64 {
        let b = body_with(md, cfg);
        let tag = &b[b.find("<wp:extent").unwrap()..];
        tag[tag.find("cx=\"").unwrap() + 4..]
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    // `width="5000"` asks for far more than any page; the clamp is the usable text width.
    let big = "<img src=\"fixture.png\" width=\"9000\">";
    let a4 = |m: &str, cfg: &Config| {
        let b = body_with(m, cfg);
        let tag = &b[b.find("<wp:extent").unwrap()..];
        tag[tag.find("cx=\"").unwrap() + 4..]
            .split('"')
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    let portrait = Config::native_math().with_base_dir(fx.dir.path());
    let landscape = with_document(|d| {
        d.page = PageSize::Custom {
            width_twips: 16838,
            height_twips: 11906,
        }
    })
    .with_base_dir(fx.dir.path());
    assert_eq!(a4(big, &portrait), (11906 - 2880) * 635, "A4 text width");
    assert_eq!(
        a4(big, &landscape),
        (16838 - 2880) * 635,
        "landscape text width is larger"
    );
    assert!(cx(&portrait) > 0);
    let docx = markdown_to_docx("hi", &landscape).unwrap();
    assert!(page_attrs(&docx).0.contains("w:w=\"16838\""));

    // Tables span the text width of the chosen page.
    let b = body_with("| a |\n|---|\n| 1 |\n", &landscape);
    assert!(b.contains("w:w=\"13958\""), "table width = 16838 - 2*1440: {b}");
}

#[test]
fn document_options_are_validated_before_any_work() {
    let bad: Vec<(&str, Config)> = vec![
        (
            "page size",
            with_document(|d| {
                d.page = PageSize::Custom {
                    width_twips: 100,
                    height_twips: 100,
                }
            }),
        ),
        (
            "page size",
            with_document(|d| {
                d.page = PageSize::Custom {
                    width_twips: 99_999,
                    height_twips: 15840,
                }
            }),
        ),
        ("margin", with_document(|d| d.margin_twips = 6000)),
        ("body size", with_document(|d| d.body_size_half_points = 0)),
        ("body size", with_document(|d| d.body_size_half_points = 10_000)),
        ("body font", with_document(|d| d.body_font = String::new())),
        ("code font", with_document(|d| d.code_font = "a\u{1}b".into())),
        ("code font", with_document(|d| d.code_font = "x".repeat(65))),
        ("heading colour", with_document(|d| d.heading_color = "red".into())),
        ("heading colour", with_document(|d| d.heading_color = "GGGGGG".into())),
    ];
    // A mermaid fence with a missing tool proves validation runs before rendering starts.
    let md = "```mermaid\ngraph TD; A-->B\n```\n";
    for (what, mut cfg) in bad {
        cfg.graphs.mermaid.program = "definitely-not-installed".into();
        match markdown_to_docx(md, &cfg) {
            Err(CrateError::InvalidConfig(m)) => assert!(m.contains(what), "`{m}` should mention `{what}`"),
            other => panic!("{what}: expected InvalidConfig, got {:?}", other.map(|d| d.len())),
        }
    }
    let mut cfg = Config::native_math();
    cfg.math.dpi = 0;
    assert!(matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(m)) if m.contains("dpi")));
    let mut cfg = Config::native_math();
    cfg.allowed_link_schemes = vec!["https:".into()];
    assert!(matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(m)) if m.contains("scheme")));
    // Sane edge values pass.
    assert!(markdown_to_docx("x", &with_document(|d| d.margin_twips = 0)).is_ok());
    assert!(markdown_to_docx("x", &with_document(|d| d.page = PageSize::Legal)).is_ok());
}

#[test]
fn builder_methods_cover_the_common_settings() {
    let mut limits = Limits::trusted();
    limits.max_input_bytes = 10;
    let cfg = Config::trusted()
        .with_math_mode(silvercity_oss::MathMode::NativeOmml)
        .with_image_access(ImageAccess::None)
        .with_html_links(false)
        .with_mermaid_engine(MermaidEngine::Mmdc)
        .with_tool_timeout(std::time::Duration::from_secs(3))
        .with_limits(limits)
        .with_base_dir("/tmp");
    assert_eq!(cfg.graphs.mermaid.program, "mmdc");
    assert_eq!(cfg.tool_timeout.as_secs(), 3);
    assert!(matches!(
        markdown_to_docx("longer than ten bytes", &cfg),
        Err(CrateError::LimitExceeded(_))
    ));
}

// ------------------------------------------------------------------- writers

#[test]
fn writer_api_streams_to_files_and_matches_the_vec_api_in_content() {
    let md = "# T\n\ntext $x^2$\n\n- a\n- b\n";
    let cfg = Config::native_math();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.docx");
    markdown_to_docx_writer(md, &cfg, std::fs::File::create(&path).unwrap()).unwrap();
    let from_file = std::fs::read(&path).unwrap();
    let from_vec = markdown_to_docx(md, &cfg).unwrap();
    assert_eq!(&from_file[..2], b"PK");
    assert_eq!(zip_entries(&from_file), zip_entries(&from_vec));
    assert_eq!(
        visible_text(&zip_text(&from_file, "word/document.xml")),
        visible_text(&zip_text(&from_vec, "word/document.xml"))
    );
}

#[test]
fn a_failed_conversion_writes_nothing() {
    let mut out = Cursor::new(Vec::new());
    assert!(markdown_to_docx_writer("a\0b", &Config::native_math(), &mut out).is_err());
    assert!(markdown_to_docx_writer("$$\\left( x$$", &Config::native_math(), &mut out).is_err());
    let mut bad = Config::native_math();
    bad.math.dpi = 0;
    assert!(markdown_to_docx_writer("x", &bad, &mut out).is_err());
    assert!(
        out.into_inner().is_empty(),
        "nothing may be written when conversion fails before packaging"
    );
}

// --------------------------------------------------------------------- files

#[test]
fn file_api_reads_safely() {
    let fx = Fixture::new();
    let cfg = Config::native_math();

    std::fs::write(fx.path("ok.md"), "# From a file\n\n![logo](fixture.png)\n").unwrap();
    let docx = markdown_file_to_docx(&fx.path("ok.md"), &cfg).unwrap();
    assert_eq!(media_count(&docx), 1, "images resolve against the file's directory");

    assert!(matches!(
        markdown_file_to_docx(&fx.path("missing.md"), &cfg),
        Err(CrateError::Io { .. })
    ));
    assert!(
        matches!(
            markdown_file_to_docx(fx.dir.path(), &cfg),
            Err(CrateError::UnsafeContent(_))
        ),
        "a directory"
    );
    std::fs::write(fx.path("bin.md"), [0xFF, 0xFE, 0xFD]).unwrap();
    assert!(matches!(
        markdown_file_to_docx(&fx.path("bin.md"), &cfg),
        Err(CrateError::InvalidMarkdown(_))
    ));

    // The size limit applies to the file *before* it is read into memory.
    let mut small = cfg.clone();
    small.limits.max_input_bytes = 8;
    assert!(matches!(
        markdown_file_to_docx(&fx.path("ok.md"), &small),
        Err(CrateError::LimitExceeded(_))
    ));
}

#[cfg(unix)]
#[test]
fn file_api_refuses_devices_instead_of_reading_forever() {
    let cfg = Config::native_math();
    for dev in ["/dev/zero", "/dev/urandom"] {
        let started = std::time::Instant::now();
        assert!(
            matches!(
                markdown_file_to_docx(std::path::Path::new(dev), &cfg),
                Err(CrateError::UnsafeContent(_))
            ),
            "{dev}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }
}

// --------------------------------------------------------------- concurrency

#[test]
fn concurrent_conversions_are_independent_and_correct() {
    let md = "# Heading\n\nLink [a](https://a.example), footnote[^1], math $x^2$.\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[^1]: note\n";
    let cfg = std::sync::Arc::new(Config::native_math());
    let results: Vec<(String, String)> = (0..16)
        .map(|_| {
            let cfg = cfg.clone();
            std::thread::spawn(move || {
                let docx = markdown_to_docx(md, &cfg).expect("conversion");
                (
                    zip_text(&docx, "word/document.xml"),
                    zip_text(&docx, "word/footnotes.xml"),
                )
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().expect("no thread may panic"))
        .collect();
    let first = visible_text(&results[0].0);
    for (body, notes) in &results {
        assert_well_formed(body);
        assert_well_formed(notes);
        assert_eq!(visible_text(body), first, "every thread produces the same content");
        assert!(visible_text(notes).contains("note"));
        assert_eq!(body.matches("<w:hyperlink").count(), 1);
    }
}

#[test]
fn config_is_send_sync_and_errors_are_std_errors() {
    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
    assert_send_sync::<Config>();
    assert_send_sync::<DocumentOptions>();
    assert_error::<CrateError>();
    // `?` conversion into Box<dyn Error> works for callers.
    fn run() -> Result<(), Box<dyn std::error::Error>> {
        markdown_to_docx("x", &Config::untrusted())?;
        Ok(())
    }
    run().unwrap();
}

#[test]
fn readers_and_files_are_read_within_the_input_limit() {
    use silvercity_oss::{analyze_file, analyze_reader, convert_reader};
    let cfg = Config::default();
    let ok = convert_reader(std::io::Cursor::new("# Hi\n\n[x](relative.md)\n"), &cfg).unwrap();
    assert!(ok.diagnostics.iter().any(|d| d.code.as_str() == "link.dropped"));
    assert!(ok.docx.starts_with(b"PK"));
    let report = analyze_reader(std::io::Cursor::new("# Hi\n"), &cfg).unwrap();
    assert_eq!(report.stats.headings, 1);

    let mut small = Config::default();
    small.limits.max_input_bytes = 10;
    // Never reads past the limit, however long the stream is.
    let endless = std::io::repeat(b'a');
    assert!(matches!(
        convert_reader(endless, &small),
        Err(CrateError::LimitExceeded(_))
    ));
    assert!(matches!(
        convert_reader(std::io::Cursor::new(vec![0xff, 0xfe]), &cfg),
        Err(CrateError::InvalidMarkdown(_))
    ));

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "# One\n\n# Two\n").unwrap();
    assert_eq!(analyze_file(&file, &cfg).unwrap().stats.headings, 2);
    assert!(matches!(
        analyze_file(&dir.path().join("missing.md"), &cfg),
        Err(CrateError::Io { .. })
    ));
}
