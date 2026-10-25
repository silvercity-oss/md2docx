//! Final packaging: docx-rs writes the archive uncompressed, so the finished
//! archive is rewritten with the configured deflate level.

use std::io::{Cursor, Seek, Write};

use docx_rs::XMLDocx;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{CrateError, Result};
use crate::safety::LimitedWriter;

fn fail(e: impl std::fmt::Display) -> CrateError {
    CrateError::DocxGenerationError(e.to_string())
}

/// Total size of the parts docx-rs built, before any compression.
pub(crate) fn uncompressed_size(parts: &XMLDocx) -> u64 {
    let media: usize = parts.media.iter().map(|(_, bytes)| bytes.len()).sum();
    [
        parts.document.len(),
        parts.styles.len(),
        parts.numberings.len(),
        parts.document_rels.len(),
        parts.footnotes.len(),
        parts.settings.len(),
        parts.content_type.len(),
        parts.rels.len(),
        parts.font_table.len(),
        parts.comments.len(),
        media,
    ]
    .iter()
    .fold(0u64, |total, n| total.saturating_add(*n as u64))
}

/// Write `parts` as a `.docx` archive to `writer`. `level` 0 stores the entries
/// as they are; 1 to 9 deflates the XML parts (embedded pictures are PNG, JPEG
/// or GIF already and stay stored). The result may not exceed `max_bytes`.
pub(crate) fn pack<W: Write + Seek>(parts: XMLDocx, level: u32, max_bytes: u64, writer: W) -> Result<()> {
    let mut stored = Cursor::new(Vec::new());
    parts.pack(&mut stored).map_err(fail)?;
    let stored = stored.into_inner();

    let mut limited = LimitedWriter::new(writer, max_bytes).map_err(fail)?;
    let written = if level == 0 {
        limited.write_all(&stored).and_then(|()| limited.flush()).map_err(fail)
    } else {
        recompress(&stored, level, &mut limited)
    };
    match written {
        Err(_) if limited.exceeded => Err(CrateError::LimitExceeded(format!(
            "the .docx would be larger than {max_bytes} bytes (Limits::max_output_bytes)"
        ))),
        other => other,
    }
}

fn recompress<W: Write + Seek>(stored: &[u8], level: u32, out: W) -> Result<()> {
    let mut archive = ZipArchive::new(Cursor::new(stored)).map_err(fail)?;
    let mut zip = ZipWriter::new(out);
    let base = SimpleFileOptions::default().unix_permissions(0o755);
    let deflated = base
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(i64::from(level)));
    let raw = base.compression_method(CompressionMethod::Stored);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(fail)?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            zip.add_directory(name, SimpleFileOptions::default()).map_err(fail)?;
            continue;
        }
        let options = if name.starts_with("word/media/") { raw } else { deflated };
        zip.start_file(name, options).map_err(fail)?;
        std::io::copy(&mut entry, &mut zip).map_err(fail)?;
    }
    zip.finish().map_err(fail)?;
    Ok(())
}
