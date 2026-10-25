//! Helpers shared by the integration test files.
#![allow(dead_code)]

use std::io::{Cursor, Read};
use std::path::PathBuf;

/// Temp directory holding `fixture.png` (a valid 600x300 PNG).
pub struct Fixture {
    pub dir: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fixture.png"), png_bytes(600, 300)).unwrap();
        Fixture { dir }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

pub fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::new(w, h)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    png.into_inner()
}

pub fn zip_entries(docx: &[u8]) -> Vec<String> {
    let z = zip::ZipArchive::new(Cursor::new(docx)).expect("output must be a zip archive");
    z.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect()
}

pub fn zip_text(docx: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(Cursor::new(docx)).unwrap();
    let mut s = String::new();
    z.by_name(name)
        .unwrap_or_else(|_| panic!("{name} missing"))
        .read_to_string(&mut s)
        .unwrap();
    // docx-rs writes empty elements as `<w:b />`; normalise so asserts can use `<w:b/>`.
    s.replace(" />", "/>")
}

pub fn media_count(docx: &[u8]) -> usize {
    zip_entries(docx)
        .iter()
        .filter(|n| n.starts_with("word/media/") && !n.ends_with('/'))
        .count()
}

/// Assert the XML part is well-formed (balanced tags, valid escapes).
pub fn assert_well_formed(xml: &str) {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_str(xml);
    r.config_mut().check_end_names = true;
    loop {
        match r.read_event() {
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("document.xml is not well-formed: {e}"),
        }
    }
}

/// Convert with the given config and return the unzipped `word/document.xml`.
pub fn body_with(md: &str, cfg: &silvercity_oss::Config) -> String {
    let docx = silvercity_oss::markdown_to_docx(md, cfg).expect("conversion should succeed");
    zip_text(&docx, "word/document.xml")
}

/// Native-math conversion; `document.xml` must also be well-formed.
pub fn body(md: &str) -> String {
    let b = body_with(md, &silvercity_oss::Config::native_math());
    assert_well_formed(&b);
    b
}

/// XML 1.0 `Char` check on a whole part: nothing Word would call corrupt.
pub fn assert_only_xml_chars(xml: &str) {
    for c in xml.chars() {
        assert!(
            matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'),
            "illegal XML character U+{:04X} in output",
            c as u32
        );
    }
}

/// Visible text of a `document.xml` part: the `<w:t>` contents, XML-unescaped, joined.
pub fn visible_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<w:t") {
        rest = &rest[i + 4..];
        if !rest.starts_with(['>', ' ']) {
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        rest = &rest[gt + 1..];
        let end = rest.find("</w:t>").unwrap_or(rest.len());
        out.push_str(&rest[..end]);
        rest = &rest[end..];
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A structurally valid PNG (correct chunk CRCs) whose header claims `w` x `h`
/// pixels, with only a token amount of image data: a decompression-bomb shape.
pub fn png_claiming(w: u32, h: u32) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &[0x78, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF]);
    chunk(&mut png, b"IEND", &[]);
    png
}

/// Write an executable `/bin/sh` script into `dir` and return its path.
#[cfg(unix)]
pub fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

/// True when `name` is on PATH.
pub fn have(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}
