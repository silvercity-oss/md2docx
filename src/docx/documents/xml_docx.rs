use super::XMLDocProps;

use crate::docx::zipper;
use std::io::prelude::*;
use std::io::Seek;

#[derive(Debug)]
/// Uncompressed OPC package parts produced by [`crate::docx::Docx::build`].
pub struct XMLDocx {
    pub content_type: Vec<u8>,
    pub rels: Vec<u8>,
    pub doc_props: XMLDocProps,
    pub styles: Vec<u8>,
    pub document: Vec<u8>,
    pub comments: Vec<u8>,
    pub document_rels: Vec<u8>,
    pub settings: Vec<u8>,
    pub font_table: Vec<u8>,
    pub numberings: Vec<u8>,
    pub media: Vec<crate::docx::ImageIdAndBuf>,
    pub headers: Vec<Vec<u8>>,
    pub header_rels: Vec<Vec<u8>>,
    pub footers: Vec<Vec<u8>>,
    pub footer_rels: Vec<Vec<u8>>,
    /// `word/_rels/footnotes.xml.rels`; empty when no footnote has a picture or link.
    pub footnote_rels: Vec<u8>,
    pub comments_extended: Vec<u8>,
    pub taskpanes: Option<Vec<u8>>,
    pub taskpanes_rels: Vec<u8>,
    pub web_extensions: Vec<Vec<u8>>,
    pub custom_items: Vec<Vec<u8>>,
    pub custom_item_rels: Vec<Vec<u8>>,
    pub custom_item_props: Vec<Vec<u8>>,
    pub footnotes: Vec<u8>,
    /// Further parts to store as-is: (path inside the package, bytes).
    pub extra_parts: Vec<(String, Vec<u8>)>,
}

impl XMLDocx {
    /// Writes these package parts as a DOCX ZIP archive to a seekable destination, deflating the XML
    /// parts at `level` (0 stores everything).
    pub fn pack<W>(self, w: W, level: u32) -> zip::result::ZipResult<()>
    where
        W: Write + Seek,
    {
        zipper::zip(w, self, level)
    }
}
