//! Host-supplied byte assets, the decode/normalise pipeline and its budgets, and accessible
//! descriptions on pictures.

mod common;

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::*;
use silvercity_oss::{markdown_to_docx, AssetMap, Config, CrateError, FenceError, ImageAccess, Limits};

fn solid(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([seed, 255 - seed, seed / 2, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn doc(md: &str, cfg: &Config) -> Vec<u8> {
    markdown_to_docx(md, cfg).unwrap_or_else(|e| panic!("conversion failed: {e}"))
}

fn body_of(docx: &[u8]) -> String {
    let b = zip_text(docx, "word/document.xml");
    assert_well_formed(&b);
    b
}

// ---- host byte assets

#[test]
fn the_host_supplies_exact_bytes_without_any_file_or_network_access() {
    let mut assets = AssetMap::new();
    assets.insert("accepted/figure-1", solid(30, 20, 10));
    assets.insert("https://example.invalid/never-fetched.png", solid(30, 20, 20));
    let cfg = Config::default().with_image_resolver(assets);
    assert_eq!(
        cfg.image_access,
        ImageAccess::None,
        "the resolver works with files disabled"
    );
    let docx = doc(
        "![One](accepted/figure-1)\n\n![Two](https://example.invalid/never-fetched.png)\n\n![Other](not-supplied.png)\n",
        &cfg,
    );
    let body = body_of(&docx);
    assert_eq!(body.matches("<w:drawing>").count(), 2, "{body}");
    assert_eq!(media_count(&docx), 2);
    assert!(
        visible_text(&body).contains("[Other]"),
        "an unsupplied key stays alt text"
    );
}

#[test]
fn a_resolver_is_asked_once_per_key_and_its_bytes_are_still_checked() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let bytes: Arc<[u8]> = solid(16, 16, 5).into();
    let cfg = Config::default().with_image_resolver(move |key: &str| -> Result<Option<Arc<[u8]>>, FenceError> {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok((key == "k").then(|| Arc::clone(&bytes)))
    });
    let docx = doc("![a](k) ![b](k) ![c](k) ![d](other)", &cfg);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one call for `k`, one for `other`");
    assert_eq!(body_of(&docx).matches("<w:drawing>").count(), 3);
    assert_eq!(media_count(&docx), 1, "identical bytes are stored once");

    // Garbage from the host is not trusted just because the host supplied it.
    let cfg = Config::default().with_image_resolver(|_: &str| -> Result<Option<Arc<[u8]>>, FenceError> {
        Ok(Some(Arc::from(&b"<svg xmlns='http://www.w3.org/2000/svg'/>"[..])))
    });
    let docx = doc("![fallback text](x)", &cfg);
    assert_eq!(media_count(&docx), 0);
    assert!(visible_text(&body_of(&docx)).contains("[fallback text]"));
}

#[test]
fn resolver_failures_and_panics_are_errors() {
    let failing = Config::default()
        .with_image_resolver(|_: &str| -> Result<Option<Arc<[u8]>>, FenceError> { Err("store offline".into()) });
    match markdown_to_docx("![a](k)", &failing) {
        Err(CrateError::ExternalToolFailed { program, detail }) => {
            assert!(program.contains("resolver") && detail.contains("store offline"));
        }
        other => panic!("{:?}", other.map(|d| d.len())),
    }
    let panicking = Config::default()
        .with_image_resolver(|_: &str| -> Result<Option<Arc<[u8]>>, FenceError> { panic!("resolver bug") });
    assert!(matches!(
        markdown_to_docx("![a](k)", &panicking),
        Err(CrateError::ExternalToolFailed { .. })
    ));
}

#[test]
fn resolved_keys_are_never_opened_as_paths() {
    let fx = Fixture::new();
    // The key looks like a path to a real file; the resolver has nothing for it.
    let cfg = Config::default().with_image_resolver(AssetMap::new());
    let docx = doc(
        &format!(
            "![x]({})",
            fx.path("fixture.png").display().to_string().replace('\\', "/")
        ),
        &cfg,
    );
    assert_eq!(media_count(&docx), 0);
}

// ---- accessible descriptions

#[test]
fn pictures_carry_alt_text_and_title_with_unique_drawing_ids() {
    let mut assets = AssetMap::new();
    assets.insert("a", solid(10, 10, 1));
    assets.insert("b", solid(10, 10, 2));
    let cfg = Config::default().with_image_resolver(assets);
    let docx = doc(
        "![A \"quoted\" <b>&amp; tricky</b> alt](a \"Figure title\")\n\n![](b)\n\n![Again](a)\n",
        &cfg,
    );
    let body = body_of(&docx);
    assert!(
        body.contains(r#"descr="A &quot;quoted&quot; &amp; tricky alt""#),
        "{body}"
    );
    assert!(body.contains(r#"title="Figure title""#), "{body}");
    let ids: Vec<&str> = body
        .match_indices("<wp:docPr id=\"")
        .map(|(i, m)| {
            let rest = &body[i + m.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    assert_eq!(ids.len(), 3);
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3, "every drawing needs its own id, got {ids:?}");
    // An image with no description gets no empty attribute.
    let second = body.split("<wp:docPr").nth(2).unwrap();
    assert!(!second[..second.find("/>").unwrap()].contains("descr="), "{second}");
}

#[test]
fn diagrams_and_formulas_are_described_by_their_source_or_the_authors_words() {
    let png = solid(40, 20, 7);
    let bytes = png.clone();
    let cfg = Config::trusted().with_fence_renderer("chart", move |_: &str| -> Result<Vec<u8>, FenceError> {
        Ok(bytes.clone())
    });
    let docx = doc(
        "```chart alt=\"Quarterly revenue by region\" title=\"Revenue\"\n1 2 3\n```\n\n```chart\nno description given\n```\n",
        &cfg,
    );
    let body = body_of(&docx);
    assert!(body.contains(r#"descr="Quarterly revenue by region""#), "{body}");
    assert!(body.contains(r#"title="Revenue""#));
    assert!(body.contains(r#"descr="Diagram: no description given""#), "{body}");
}

// ---- decode, normalise, budgets

#[test]
fn every_format_is_decoded_and_embedded_as_a_clean_png() {
    let mut padded = solid(12, 12, 9);
    padded.extend_from_slice(b"APPENDED-SECRET-PAYLOAD");
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(20, 10, image::Rgb([9, 99, 199])))
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let mut gif = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255])))
        .write_to(&mut gif, image::ImageFormat::Gif)
        .unwrap();
    let mut assets = AssetMap::new();
    assets.insert("png", padded);
    assets.insert("jpeg", jpeg.into_inner());
    assets.insert("gif", gif.into_inner());
    let cfg = Config::default().with_image_resolver(assets);
    let docx = doc("![](png) ![](jpeg) ![](gif)", &cfg);
    assert_eq!(media_count(&docx), 3);
    for name in zip_entries(&docx)
        .into_iter()
        .filter(|n| n.starts_with("word/media/") && n.ends_with(".png"))
    {
        let raw = zip_bytes(&docx, &name);
        assert!(raw.starts_with(&[0x89, b'P', b'N', b'G']), "{name} is not a PNG");
        assert!(!raw.windows(8).any(|w| w == b"APPENDED"), "{name} kept trailing data");
    }
}

/// The limits an application with a strict profile might set; nothing here is a crate default.
fn strict_profile() -> Config {
    let mut l = Limits::default();
    l.max_image_bytes = 1024 * 1024;
    l.max_distinct_images = 50;
    l.max_image_pixels = 4_000_000;
    l.max_total_decode_pixels = 16_000_000;
    l.max_total_normalized_bytes = 8 * 1024 * 1024;
    l.max_output_bytes = 16 * 1024 * 1024;
    Config::default().with_limits(l)
}

#[test]
fn a_strict_profile_is_expressible_and_enforced_at_its_boundaries() {
    // 50 distinct images pass, the 51st does not; repeats are free.
    let mut assets = AssetMap::new();
    let mut md = String::new();
    for i in 0..51u8 {
        assets.insert(format!("i{i}"), solid(8, 8, i));
        md.push_str(&format!("![](i{i}) "));
    }
    let cfg = strict_profile().with_image_resolver(assets.clone());
    match markdown_to_docx(&md, &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("distinct images"), "{m}"),
        other => panic!("{:?}", other.map(|d| d.len())),
    }
    let first_fifty: String = (0..50).map(|i| format!("![](i{i}) ![](i{i}) ")).collect();
    assert!(
        markdown_to_docx(&first_fifty, &cfg).is_ok(),
        "50 distinct encodings, each used twice"
    );

    // 4 million pixels per image: exactly 4M passes, one more does not.
    let mut big = AssetMap::new();
    big.insert("ok", solid(2000, 2000, 1));
    big.insert("over", solid(2000, 2001, 2));
    let cfg = strict_profile().with_image_resolver(big);
    assert!(markdown_to_docx("![](ok)", &cfg).is_ok());
    assert!(matches!(
        markdown_to_docx("![](over)", &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("pixels")
    ));
}

#[test]
fn decode_work_is_charged_per_distinct_encoding_even_for_identical_pixels() {
    // Four different encodings of the same 2000x2000 pixels cost 16M decoded pixels; a fifth
    // does not fit, however small the output is.
    let pixels = image::RgbaImage::from_pixel(2000, 2000, image::Rgba([40, 80, 120, 255]));
    let encode = |level: image::codecs::png::CompressionType, filter: image::codecs::png::FilterType| {
        use image::ImageEncoder;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(&mut out, level, filter)
            .write_image(pixels.as_raw(), 2000, 2000, image::ExtendedColorType::Rgba8)
            .unwrap();
        out
    };
    use image::codecs::png::{CompressionType as C, FilterType as F};
    let variants = [
        encode(C::Default, F::NoFilter),
        encode(C::Default, F::Sub),
        encode(C::Default, F::Up),
        encode(C::Default, F::Avg),
        encode(C::Default, F::Paeth),
    ];
    for (i, v) in variants.iter().enumerate() {
        assert_ne!(v, &variants[(i + 1) % variants.len()], "the encodings must differ");
    }
    let mut assets = AssetMap::new();
    for (i, v) in variants.iter().enumerate() {
        assets.insert(format!("v{i}"), v.clone());
    }
    let cfg = strict_profile().with_image_resolver(assets);
    let r = markdown_to_docx("![](v0) ![](v1) ![](v2) ![](v3)", &cfg);
    assert!(r.is_ok(), "{:?}", r.map(|d| d.len()));
    assert!(
        markdown_to_docx("![](v0) ![](v0) ![](v0) ![](v0) ![](v0) ![](v0)", &cfg).is_ok(),
        "repeats reuse"
    );
    match markdown_to_docx("![](v0) ![](v1) ![](v2) ![](v3) ![](v4)", &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("decoding images"), "{m}"),
        other => panic!("{:?}", other.map(|d| d.len())),
    }
}

#[test]
fn normalised_png_bytes_are_an_aggregate_budget() {
    // Noisy images do not compress: a few of them exhaust a small normalised-bytes budget.
    let noisy = |seed: u32| {
        let mut img = image::RgbaImage::new(128, 128);
        let mut x = seed.wrapping_mul(2654435761).max(1);
        for p in img.pixels_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *p = image::Rgba([x as u8, (x >> 8) as u8, (x >> 16) as u8, 255]);
        }
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let mut assets = AssetMap::new();
    for i in 0..4 {
        assets.insert(format!("n{i}"), noisy(i + 1));
    }
    let mut cfg = strict_profile().with_image_resolver(assets);
    cfg.limits.max_total_normalized_bytes = 140_000;
    assert!(markdown_to_docx("![](n0) ![](n1)", &cfg).is_ok());
    assert!(matches!(
        markdown_to_docx("![](n0) ![](n1) ![](n2) ![](n3)", &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("normalised")
    ));
}
