//! Final packaging: the archive is written once, with the configured deflate level, into a
//! size-capped buffer, and handed to the caller's writer only if it fits.

use std::io::{Cursor, Seek, Write};

use crate::docx::XMLDocx;
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

/// Write `parts` as a `.docx` archive to `writer`. `level` 0 stores the entries;
/// 1 to 9 deflates the XML parts (embedded pictures are PNG already and stay
/// stored). The archive is built in memory, capped at `max_bytes`, and handed to
/// `writer` in one piece: a refused document writes nothing.
pub(crate) fn pack<W: Write + Seek>(parts: XMLDocx, level: u32, max_bytes: u64, mut writer: W) -> Result<()> {
    let mut limited = LimitedWriter::new(Cursor::new(Vec::new()), max_bytes).map_err(fail)?;
    match parts.pack(&mut limited, level) {
        Err(_) if limited.exceeded => {
            return Err(CrateError::LimitExceeded(format!(
                "the .docx would be larger than {max_bytes} bytes (Limits::max_output_bytes)"
            )))
        }
        other => other.map_err(fail)?,
    }
    let archive = limited.into_inner().into_inner();
    writer.write_all(&archive).and_then(|()| writer.flush()).map_err(fail)
}
