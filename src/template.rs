//! Reference documents: take the look of a document from a `.docx` you supply.
//!
//! A reference document supplies its **styles** (`word/styles.xml`: fonts, sizes, colours,
//! spacing, heading looks, the default paragraph) and its **theme** (`word/theme/theme1.xml`:
//! the colour and font scheme those styles may refer to). The text of the reference is never used.
//! The converter writes paragraphs with the style ids Word itself uses (`Heading1`..`Heading6`,
//! `Hyperlink`, `FootnoteReference`) and a few of its own (`InlineCode`, `CodeBlock`); a style the
//! reference does not define is added from the converter's own definition, so any `.docx` works as
//! a reference, and one that defines these ids restyles the output completely.
//!
//! The reference is **trusted configuration** (you chose it), but it is still read defensively:
//! bounded sizes, a bounded number of archive entries, and only two entries are ever read.

use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::error::{CrateError, Result};

/// Largest reference document accepted.
const MAX_REFERENCE_BYTES: usize = 16 * 1024 * 1024;
/// Largest `styles.xml` or theme part read from it.
const MAX_PART_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 2_000;
/// Most styles taken from a reference.
const MAX_STYLES: usize = 5_000;

/// The parts of a reference document the converter uses. Build it once with
/// [`ReferenceDocx::from_bytes`] and attach it with
/// [`Config::with_reference_docx`](crate::Config::with_reference_docx).
#[derive(Clone)]
pub struct ReferenceDocx {
    pub(crate) styles: String,
    pub(crate) theme: Option<Vec<u8>>,
}

impl std::fmt::Debug for ReferenceDocx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceDocx")
            .field("styles_bytes", &self.styles.len())
            .field("theme", &self.theme.is_some())
            .finish()
    }
}

pub(crate) fn bad(why: impl std::fmt::Display) -> CrateError {
    CrateError::InvalidConfig(format!("reference document: {why}"))
}

fn read_entry<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Option<Vec<u8>>> {
    let entry = match archive.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(bad(e)),
    };
    if entry.size() > MAX_PART_BYTES {
        return Err(bad(format!("{name} is larger than {MAX_PART_BYTES} bytes")));
    }
    let mut out = Vec::new();
    // The declared size is only a claim: read at most the limit regardless.
    entry.take(MAX_PART_BYTES + 1).read_to_end(&mut out).map_err(bad)?;
    if out.len() as u64 > MAX_PART_BYTES {
        return Err(bad(format!("{name} is larger than {MAX_PART_BYTES} bytes")));
    }
    Ok(Some(out))
}

/// Check that `xml` is well-formed XML (balanced elements, valid escapes, no DOCTYPE).
fn well_formed(xml: &str, what: &str) -> Result<()> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(_)) => depth += 1,
            Ok(quick_xml::events::Event::End(_)) => depth = depth.saturating_sub(1),
            // Entity declarations are the way XML expands small input into large; refuse them.
            Ok(quick_xml::events::Event::DocType(_)) => return Err(bad(format!("{what} has a DOCTYPE"))),
            Ok(_) => {}
            Err(e) => return Err(bad(format!("{what} is not well-formed XML: {e}"))),
        }
    }
    if depth != 0 {
        return Err(bad(format!("{what} has unclosed elements")));
    }
    Ok(())
}

impl ReferenceDocx {
    /// Read the styles (and theme, when present) of the `.docx` in `bytes`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_REFERENCE_BYTES {
            return Err(bad(format!("larger than {MAX_REFERENCE_BYTES} bytes")));
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(format!("not a .docx archive ({e})")))?;
        if archive.len() > MAX_ENTRIES {
            return Err(bad(format!("more than {MAX_ENTRIES} archive entries")));
        }
        let styles = read_entry(&mut archive, "word/styles.xml")?.ok_or_else(|| bad("it has no word/styles.xml"))?;
        let styles = String::from_utf8(styles).map_err(|_| bad("word/styles.xml is not UTF-8"))?;
        let theme = read_entry(&mut archive, "word/theme/theme1.xml")?;
        Self::from_parts(styles, theme)
    }

    /// Check `styles` (a whole `styles.xml`) and `theme` and keep them.
    pub(crate) fn from_parts(styles: String, theme: Option<Vec<u8>>) -> Result<Self> {
        well_formed(&styles, "word/styles.xml")?;
        if !styles.contains("<w:styles") {
            return Err(bad("word/styles.xml has no <w:styles> root"));
        }
        if let Some(t) = &theme {
            let text = std::str::from_utf8(t).map_err(|_| bad("the theme is not UTF-8"))?;
            well_formed(text, "word/theme/theme1.xml")?;
        }
        // Every style block must be findable now, so merging later cannot fail.
        style_blocks(&styles)?;
        Ok(ReferenceDocx { styles, theme })
    }
}

/// `(styleId, whole <w:style>...</w:style> text)` for every style in `xml`.
pub(crate) fn style_blocks(xml: &str) -> Result<Vec<(String, &str)>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = xml[from..].find("<w:style ") {
        let start = from + rel;
        let end = xml[start..]
            .find("</w:style>")
            .map(|e| start + e + "</w:style>".len())
            .ok_or_else(|| bad("a <w:style> element is not closed"))?;
        let block = &xml[start..end];
        let head_end = block.find('>').unwrap_or(block.len());
        let head = &block[..head_end];
        if let Some(at) = head.find("w:styleId=\"") {
            let rest = &head[at + "w:styleId=\"".len()..];
            if let Some(q) = rest.find('"') {
                out.push((rest[..q].to_string(), block));
            }
        }
        if out.len() > MAX_STYLES {
            return Err(bad(format!("more than {MAX_STYLES} styles")));
        }
        from = end;
    }
    Ok(out)
}

/// The reference's styles, plus every style of `generated` the reference does not define.
pub(crate) fn merge_styles(reference: &ReferenceDocx, generated: &str) -> Result<String> {
    let have: std::collections::HashSet<String> =
        style_blocks(&reference.styles)?.into_iter().map(|(id, _)| id).collect();
    let missing: String = style_blocks(generated)?
        .into_iter()
        .filter(|(id, _)| !have.contains(id))
        .map(|(_, block)| block)
        .collect();
    let close = reference
        .styles
        .rfind("</w:styles>")
        .ok_or_else(|| bad("word/styles.xml has no closing </w:styles>"))?;
    let mut out = String::with_capacity(reference.styles.len() + missing.len());
    out.push_str(&reference.styles[..close]);
    out.push_str(&missing);
    out.push_str(&reference.styles[close..]);
    Ok(out)
}

/// Add the theme part to the package: the file, its content type and the relationship to it.
pub(crate) fn add_theme(parts: &mut crate::docx::XMLDocx, theme: &[u8]) -> Result<()> {
    let edit = |bytes: &mut Vec<u8>, before: &str, insert: &str| -> Result<()> {
        let text = std::str::from_utf8(bytes).map_err(|_| bad("package part is not UTF-8"))?;
        let at = text
            .rfind(before)
            .ok_or_else(|| bad(format!("package part has no {before}")))?;
        let mut out = String::with_capacity(text.len() + insert.len());
        out.push_str(&text[..at]);
        out.push_str(insert);
        out.push_str(&text[at..]);
        *bytes = out.into_bytes();
        Ok(())
    };
    edit(
        &mut parts.content_type,
        "</Types>",
        "<Override PartName=\"/word/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>",
    )?;
    edit(
        &mut parts.document_rels,
        "</Relationships>",
        "<Relationship Id=\"rIdTheme1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/>",
    )?;
    parts
        .extra_parts
        .push(("word/theme/theme1.xml".to_string(), theme.to_vec()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = "<w:styles xmlns:w=\"w\"><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/></w:style></w:styles>";

    #[test]
    fn missing_styles_are_appended_and_present_ones_are_kept() {
        let r = ReferenceDocx {
            styles: SMALL.to_string(),
            theme: None,
        };
        let generated = "<w:styles><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"OURS\"/></w:style><w:style w:type=\"character\" w:styleId=\"InlineCode\"><w:name w:val=\"code\"/></w:style></w:styles>";
        let merged = merge_styles(&r, generated).unwrap();
        assert!(merged.contains("heading 1") && !merged.contains("OURS"), "{merged}");
        assert!(merged.contains("w:styleId=\"InlineCode\""));
        assert!(merged.ends_with("</w:styles>"));
        well_formed(&merged, "merged").unwrap();
    }

    #[test]
    fn a_reference_must_be_a_docx_with_well_formed_styles() {
        assert!(ReferenceDocx::from_bytes(b"not a zip").is_err());
        assert!(ReferenceDocx::from_bytes(&vec![0u8; MAX_REFERENCE_BYTES + 1]).is_err());
        assert!(well_formed("<a><b></a>", "x").is_err());
        assert!(well_formed("<!DOCTYPE x [<!ENTITY a \"b\">]><x/>", "x").is_err());
        assert!(style_blocks("<w:style w:styleId=\"A\">").is_err());
    }
}
