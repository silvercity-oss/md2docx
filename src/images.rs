//! Every raster image that reaches the document goes through here: it is measured, decoded in
//! full, re-encoded as a plain 8-bit PNG, and charged against the document's image budgets.
//!
//! Decoding everything (PNG included) means a file that merely *claims* to be a PNG, or carries
//! trailing data, ancillary chunks or a malformed stream, never reaches Word as-is, and that
//! the decode work, not just the file size, is what the limits bound.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::{DynamicImage, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};

use crate::config::Limits;
use crate::control::Ctl;
use crate::error::{CrateError, Result};

/// A decoded and re-encoded picture: what is embedded in the `.docx`.
#[derive(Debug)]
pub(crate) struct Picture {
    pub png: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

/// Why an image could not be embedded. These are not limit violations: the caller shows the
/// alt text instead and reports a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// SVG, WebP, TIFF, ... : not a format this build embeds.
    Format,
    /// Recognised but undecodable (truncated, corrupt, inconsistent header).
    Corrupt,
    /// A zero-sized image.
    Empty,
}

impl Unsupported {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Unsupported::Format => "not a PNG, JPEG or GIF image",
            Unsupported::Corrupt => "the image data could not be decoded",
            Unsupported::Empty => "the image has no pixels",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Outcome {
    Picture(Arc<Picture>),
    Unsupported(Unsupported),
}

/// Budgets and the exact-repeat cache of one conversion.
#[derive(Debug, Default)]
pub(crate) struct ImagePipeline {
    /// SHA-256 of the source encoding -> what it normalised to. An exact repeat reuses this and
    /// is charged nothing: it does no new decode work and adds no new bytes to the document.
    seen: HashMap<[u8; 32], Outcome>,
    source_bytes: u64,
    decoded_pixels: u64,
    normalized_bytes: u64,
}

fn limit(what: String) -> CrateError {
    CrateError::LimitExceeded(what)
}

impl ImagePipeline {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Measure, decode and re-encode `bytes`, enforcing every image limit. `origin` names the
    /// image in error messages (a path, a renderer, an asset key).
    ///
    /// Charges, in order, **before** the work they pay for: the distinct-image count and source
    /// bytes, the header's claimed size against the per-image limits, the pixels against the
    /// aggregate decode budget, and finally the resulting PNG against the normalised-bytes
    /// budget. Different encodings of identical pixels are different sources and each pays.
    pub(crate) fn normalize(&mut self, limits: &Limits, ctl: &Ctl, bytes: &[u8], origin: &str) -> Result<Outcome> {
        if bytes.len() as u64 > limits.max_image_bytes {
            return Err(limit(format!(
                "image {origin} is {} bytes (limit {})",
                bytes.len(),
                limits.max_image_bytes
            )));
        }
        let key: [u8; 32] = Sha256::digest(bytes).into();
        if let Some(done) = self.seen.get(&key) {
            return Ok(done.clone());
        }
        ctl.check()?;
        if self.seen.len() >= limits.max_distinct_images {
            return Err(limit(format!(
                "more than {} distinct images in one document (while reading {origin})",
                limits.max_distinct_images
            )));
        }
        self.source_bytes = self.source_bytes.saturating_add(bytes.len() as u64);
        if self.source_bytes > limits.max_total_image_bytes {
            return Err(limit(format!(
                "images exceed {} source bytes in total (while reading {origin})",
                limits.max_total_image_bytes
            )));
        }

        let outcome = self.decode(limits, ctl, bytes, origin)?;
        self.seen.insert(key, outcome.clone());
        Ok(outcome)
    }

    fn decode(&mut self, limits: &Limits, ctl: &Ctl, bytes: &[u8], origin: &str) -> Result<Outcome> {
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| CrateError::io(origin, e))?;
        // Only the formats this build can decode are recognised; anything else is "unsupported".
        match reader.format() {
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif) => {}
            _ => return Ok(Outcome::Unsupported(Unsupported::Format)),
        }
        // The header's claim, judged before anything is allocated for it.
        let Ok((w, h)) = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| CrateError::io(origin, e))?
            .into_dimensions()
        else {
            return Ok(Outcome::Unsupported(Unsupported::Corrupt));
        };
        if w == 0 || h == 0 {
            return Ok(Outcome::Unsupported(Unsupported::Empty));
        }
        let pixels = u64::from(w) * u64::from(h);
        if pixels > limits.max_image_pixels {
            return Err(limit(format!(
                "image {origin} is {w}x{h} pixels (limit {} pixels)",
                limits.max_image_pixels
            )));
        }
        if w.max(h) > limits.max_image_dimension {
            return Err(limit(format!(
                "image {origin} is {w}x{h} pixels (longest side limit {})",
                limits.max_image_dimension
            )));
        }
        self.decoded_pixels = self.decoded_pixels.saturating_add(pixels);
        if self.decoded_pixels > limits.max_total_decode_pixels {
            return Err(limit(format!(
                "decoding images would exceed {} pixels in total (while reading {origin})",
                limits.max_total_decode_pixels
            )));
        }

        let mut reader = reader;
        let mut decode_limits = image::Limits::default();
        decode_limits.max_alloc = Some(limits.max_decode_alloc_bytes);
        decode_limits.max_image_width = Some(limits.max_image_dimension);
        decode_limits.max_image_height = Some(limits.max_image_dimension);
        reader.limits(decode_limits);
        let Ok(img) = reader.decode() else {
            return Ok(Outcome::Unsupported(Unsupported::Corrupt));
        };
        ctl.check()?;

        // One plain 8-bit form per colour model: gray, RGB or RGBA.
        let img = if img.color().has_alpha() {
            DynamicImage::ImageRgba8(img.into_rgba8())
        } else if img.color().has_color() {
            DynamicImage::ImageRgb8(img.into_rgb8())
        } else {
            DynamicImage::ImageLuma8(img.into_luma8())
        };
        let mut out = Cursor::new(Vec::new());
        if img.write_to(&mut out, ImageFormat::Png).is_err() {
            return Ok(Outcome::Unsupported(Unsupported::Corrupt));
        }
        let png = out.into_inner();
        self.normalized_bytes = self.normalized_bytes.saturating_add(png.len() as u64);
        if self.normalized_bytes > limits.max_total_normalized_bytes {
            return Err(limit(format!(
                "normalised images exceed {} bytes in total (while reading {origin})",
                limits.max_total_normalized_bytes
            )));
        }
        Ok(Outcome::Picture(Arc::new(Picture {
            png: png.into(),
            width: w,
            height: h,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w, h);
        for (i, p) in img.pixels_mut().enumerate() {
            *p = image::Rgba([seed, (i % 251) as u8, (i / 7 % 255) as u8, 255]);
        }
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut out, ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    fn tight() -> Limits {
        let mut l = Limits::trusted();
        l.max_distinct_images = 3;
        l.max_total_decode_pixels = 10_000;
        l
    }

    #[test]
    fn exact_repeats_are_free_but_different_encodings_pay_for_the_decode() {
        let mut p = ImagePipeline::new();
        let mut l = tight();
        l.max_total_decode_pixels = 2 * 40 * 40; // room for two 40x40 decodes
        let a = png(40, 40, 1);
        for _ in 0..50 {
            assert!(matches!(
                p.normalize(&l, &Ctl::none(), &a, "a").unwrap(),
                Outcome::Picture(_)
            ));
        }
        assert_eq!(p.decoded_pixels, 1600, "50 exact repeats decode once");
        // A different encoding of the very same pixels is a new source and is charged.
        let mut re = Cursor::new(Vec::new());
        let img = image::load_from_memory(&a).unwrap();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut re,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        )
        .write_image(img.as_bytes(), 40, 40, img.color().into())
        .unwrap();
        let re = re.into_inner();
        assert_ne!(re, a);
        p.normalize(&l, &Ctl::none(), &re, "b").unwrap();
        assert_eq!(p.decoded_pixels, 3200);
        let third = png(40, 40, 2);
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), &third, "c"),
            Err(CrateError::LimitExceeded(m)) if m.contains("decoding images")
        ));
    }

    #[test]
    fn distinct_image_count_counts_encodings_not_uses() {
        let mut p = ImagePipeline::new();
        let l = tight();
        for seed in 0..3 {
            p.normalize(&l, &Ctl::none(), &png(8, 8, seed), "x").unwrap();
        }
        // Repeats of any of the three are still fine ...
        p.normalize(&l, &Ctl::none(), &png(8, 8, 1), "x").unwrap();
        // ... a fourth distinct encoding is not.
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), &png(8, 8, 9), "x"),
            Err(CrateError::LimitExceeded(m)) if m.contains("distinct images")
        ));
    }

    #[test]
    fn per_image_limits_are_judged_from_the_header() {
        let mut l = Limits::trusted();
        l.max_image_pixels = 100;
        let mut p = ImagePipeline::new();
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), &png(11, 10, 0), "big"),
            Err(CrateError::LimitExceeded(m)) if m.contains("pixels")
        ));
        assert_eq!(p.decoded_pixels, 0, "nothing was charged or decoded for it");
        let mut l = Limits::trusted();
        l.max_image_dimension = 16;
        assert!(matches!(
            ImagePipeline::new().normalize(&l, &Ctl::none(), &png(17, 1, 0), "wide"),
            Err(CrateError::LimitExceeded(m)) if m.contains("longest side")
        ));
        let mut l = Limits::trusted();
        l.max_image_bytes = 10;
        assert!(matches!(
            ImagePipeline::new().normalize(&l, &Ctl::none(), &png(8, 8, 0), "heavy"),
            Err(CrateError::LimitExceeded(m)) if m.contains("bytes")
        ));
    }

    #[test]
    fn normalised_bytes_are_an_aggregate_budget() {
        let mut l = Limits::trusted();
        l.max_total_normalized_bytes = 300;
        let mut p = ImagePipeline::new();
        let r = p.normalize(&l, &Ctl::none(), &png(64, 64, 3), "noisy");
        assert!(matches!(r, Err(CrateError::LimitExceeded(m)) if m.contains("normalised")));
    }

    #[test]
    fn everything_is_re_encoded_and_unsupported_input_is_reported_not_embedded() {
        let mut p = ImagePipeline::new();
        let l = Limits::trusted();
        let original = png(5, 5, 4);
        let Outcome::Picture(pic) = p.normalize(&l, &Ctl::none(), &original, "png").unwrap() else {
            panic!("png")
        };
        assert_eq!((pic.width, pic.height), (5, 5));
        // A PNG with junk appended decodes, and the junk does not survive normalisation.
        let mut padded = original.clone();
        padded.extend_from_slice(b"TRAILING-JUNK");
        let Outcome::Picture(clean) = p.normalize(&l, &Ctl::none(), &padded, "padded").unwrap() else {
            panic!("padded")
        };
        assert!(!clean.png.windows(8).any(|w| w == b"TRAILING"));
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), &jpeg(9, 9), "jpeg").unwrap(),
            Outcome::Picture(_)
        ));
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), b"<svg xmlns='http://www.w3.org/2000/svg'/>", "svg")
                .unwrap(),
            Outcome::Unsupported(Unsupported::Format)
        ));
        // Claims to be a PNG, is not one.
        let mut broken = original[..40].to_vec();
        broken.truncate(40);
        assert!(matches!(
            p.normalize(&l, &Ctl::none(), &broken, "truncated").unwrap(),
            Outcome::Unsupported(_)
        ));
    }
}
