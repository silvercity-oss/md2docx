//! Representative documents for manual and automated Word/LibreOffice checks.
//!
//! The Markdown sources live in `tests/fixtures/`; this builds the configuration that turns them
//! into `.docx` files (host-supplied images, a drawn diagram unless the `mermaid` feature renders
//! a real one). Shared by `tests/office_roundtrip.rs` and `examples/fixtures.rs`.
#![allow(dead_code)]

use std::io::Cursor;

use silvercity_oss::{AssetMap, Config, FenceError};

/// (file stem, Markdown source, whether the strict policy must accept it).
pub const SOURCES: &[(&str, &str, bool)] = &[
    ("01-structure", include_str!("../fixtures/01-structure.md"), true),
    ("02-math", include_str!("../fixtures/02-math.md"), true),
    ("03-images", include_str!("../fixtures/03-images.md"), true),
    ("04-footnotes", include_str!("../fixtures/04-footnotes.md"), true),
    ("05-diagram", include_str!("../fixtures/05-diagram.md"), true),
    ("06-lossy", include_str!("../fixtures/06-lossy.md"), false),
];

fn png(width: u32, height: u32, hue: u8) -> Vec<u8> {
    let mut img = image::RgbaImage::new(width, height);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let on_border = x < 3 || y < 3 || x + 3 >= width || y + 3 >= height;
        *p = if on_border {
            image::Rgba([20, 20, 20, 255])
        } else {
            image::Rgba([
                hue.wrapping_add((x / 8) as u8),
                200u8.wrapping_sub((y / 6) as u8),
                (hue / 2).wrapping_add(90),
                255,
            ])
        };
    }
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).expect("encode png");
    out.into_inner()
}

/// The images the fixtures refer to, by key.
pub fn assets() -> AssetMap {
    let mut a = AssetMap::new();
    a.insert("assets/architecture", png(480, 240, 30));
    a.insert("assets/chart", png(320, 200, 140));
    a
}

/// The configuration the fixtures are built with: the untrusted-input default, plus the host's
/// images and a diagram renderer.
pub fn config() -> Config {
    let cfg = Config::default().with_image_resolver(assets());
    #[cfg(feature = "mermaid")]
    {
        cfg.with_in_process_mermaid()
    }
    #[cfg(not(feature = "mermaid"))]
    {
        cfg.with_fence_renderer("mermaid", |_: &str| -> Result<Vec<u8>, FenceError> {
            Ok(png(400, 160, 200))
        })
    }
}
