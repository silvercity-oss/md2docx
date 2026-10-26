//! Compression level, and the output and memory limits that depend on it.

mod common;

use std::io::{Cursor, Read};

use common::*;
use md2docx::{markdown_to_docx, markdown_to_docx_writer, Config, CrateError, ImageAccess};

fn repetitive_markdown() -> String {
    "```\n".to_string() + &"line of code that repeats\n".repeat(3000) + "```\n\n" + &"Some **bold** prose. ".repeat(300)
}

fn cfg() -> Config {
    Config::native_math().with_image_access(ImageAccess::None)
}

fn size_at(md: &str, level: u32) -> usize {
    markdown_to_docx(md, &cfg().with_compression_level(level))
        .unwrap()
        .len()
}

#[test]
fn default_level_is_6_and_compresses_the_xml() {
    assert_eq!(Config::trusted().compression_level, 6);
    assert_eq!(Config::untrusted().compression_level, 6);
    let md = repetitive_markdown();
    let default = markdown_to_docx(&md, &cfg()).unwrap().len();
    // (Ids come from process-wide counters, so two calls can differ by a few bytes.)
    assert!(default.abs_diff(size_at(&md, 6)) < 64);
    let stored = size_at(&md, 0);
    assert!(default * 8 < stored, "level 6 gave {default} bytes, stored {stored}");
}

#[test]
fn levels_order_by_size_and_every_level_gives_the_same_document() {
    let md = repetitive_markdown();
    let sizes: Vec<usize> = (0..=9).map(|l| size_at(&md, l)).collect();
    assert!(sizes[1] < sizes[0] && sizes[9] <= sizes[1], "{sizes:?}");
    assert!(sizes[6] <= sizes[1] && sizes[9] <= sizes[6], "{sizes:?}");

    let reference = zip_text(
        &markdown_to_docx(&md, &cfg().with_compression_level(0)).unwrap(),
        "word/document.xml",
    );
    for level in 1..=9 {
        let docx = markdown_to_docx(&md, &cfg().with_compression_level(level)).unwrap();
        // Ids come from process-wide counters, so compare the visible content.
        assert_eq!(
            visible_text(&zip_text(&docx, "word/document.xml")),
            visible_text(&reference),
            "level {level}"
        );
        assert_well_formed(&zip_text(&docx, "word/document.xml"));
    }
}

#[test]
fn the_archive_layout_is_unchanged_and_pictures_stay_stored() {
    let fx = Fixture::new();
    let c = cfg()
        .with_image_access(ImageAccess::BaseDir)
        .with_base_dir(fx.dir.path());
    let md = "![pic](fixture.png)\n\ntext\n";
    let stored = markdown_to_docx(md, &c.clone().with_compression_level(0)).unwrap();
    let packed = markdown_to_docx(md, &c).unwrap();
    // Picture names carry a process-wide counter, so compare them by prefix.
    let layout = |d: &[u8]| -> Vec<String> {
        zip_entries(d)
            .into_iter()
            .map(|n| {
                if n.starts_with("word/media/") && !n.ends_with('/') {
                    "word/media/*".into()
                } else {
                    n
                }
            })
            .collect()
    };
    assert_eq!(layout(&stored), layout(&packed), "same entries in the same order");
    assert_eq!(zip_entries(&packed)[0], "word/", "docx-rs order is kept");
    let mut z = zip::ZipArchive::new(Cursor::new(&packed)).unwrap();
    for i in 0..z.len() {
        let f = z.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let expect_stored = f.is_dir() || name.starts_with("word/media/");
        assert_eq!(
            f.compression() == zip::CompressionMethod::Stored,
            expect_stored,
            "{name}: {:?}",
            f.compression()
        );
    }
    // The picture is byte-for-byte the original.
    let mut original = Vec::new();
    std::fs::File::open(fx.path("fixture.png"))
        .unwrap()
        .read_to_end(&mut original)
        .unwrap();
    let media = zip_entries(&packed)
        .into_iter()
        .find(|n| n.starts_with("word/media/") && !n.ends_with('/'))
        .unwrap();
    let mut z = zip::ZipArchive::new(Cursor::new(&packed)).unwrap();
    let mut out = Vec::new();
    z.by_name(&media).unwrap().read_to_end(&mut out).unwrap();
    assert_eq!(out, original);
}

#[test]
fn the_level_is_validated() {
    assert!(matches!(
        markdown_to_docx("x", &cfg().with_compression_level(10)),
        Err(CrateError::InvalidConfig(m)) if m.contains("compression_level")
    ));
    assert!(markdown_to_docx("x", &cfg().with_compression_level(9)).is_ok());
}

#[test]
fn the_output_limit_applies_to_the_compressed_file_and_is_exact() {
    let md = repetitive_markdown();
    let full = size_at(&md, 6) as u64;
    // 80 KB of XML fits a 40 KB limit once compressed; stored it would not.
    assert!(size_at(&md, 0) as u64 > 2 * full);

    // At the limit (give or take the few bytes ids can vary by) it fits, under it does not.
    let mut at = cfg();
    at.limits.max_output_bytes = full + 64;
    let len = markdown_to_docx(&md, &at).unwrap().len() as u64;
    assert!(len <= full + 64 && len + 64 >= full, "{len} vs {full}");

    let mut under = cfg();
    under.limits.max_output_bytes = full - 64;
    assert!(matches!(
        markdown_to_docx(&md, &under),
        Err(CrateError::LimitExceeded(m)) if m.contains("max_output_bytes")
    ));

    // The same limit with level 0 refuses, because the stored archive is larger.
    let mut stored = cfg().with_compression_level(0);
    stored.limits.max_output_bytes = full;
    assert!(matches!(
        markdown_to_docx(&md, &stored),
        Err(CrateError::LimitExceeded(_))
    ));

    // A caller's buffer receives the whole archive or nothing.
    let mut small = cfg();
    small.limits.max_output_bytes = 2048;
    let mut sink = Cursor::new(Vec::new());
    assert!(matches!(
        markdown_to_docx_writer(&md, &small, &mut sink),
        Err(CrateError::LimitExceeded(_))
    ));
    assert!(
        sink.get_ref().is_empty(),
        "a refused document writes nothing, got {} bytes",
        sink.get_ref().len()
    );
}

#[test]
fn the_uncompressed_limit_bounds_memory_before_any_packaging() {
    let md = repetitive_markdown();
    let mut c = cfg();
    c.limits.max_uncompressed_bytes = 10_000;
    assert!(matches!(
        markdown_to_docx(&md, &c),
        Err(CrateError::LimitExceeded(m)) if m.contains("max_uncompressed_bytes")
    ));
    let mut c = cfg();
    c.limits.max_uncompressed_bytes = 10_000_000;
    assert!(markdown_to_docx(&md, &c).is_ok());
}

#[test]
fn untrusted_caps() {
    let l = Config::untrusted().limits;
    assert_eq!((l.max_output_bytes, l.max_uncompressed_bytes), (50 << 20, 64 << 20));
    assert_eq!(l.max_elements, 60_000);
    let l = Config::untrusted_with_diagrams().limits;
    assert_eq!((l.max_output_bytes, l.max_uncompressed_bytes), (50 << 20, 64 << 20));
    let d = Config::trusted().limits;
    assert!(d.max_output_bytes > 50 << 20 && d.max_uncompressed_bytes > 64 << 20 && d.max_elements > 60_000);
}
