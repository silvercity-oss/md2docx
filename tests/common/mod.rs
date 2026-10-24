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
    z.file_names().map(str::to_string).collect()
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
