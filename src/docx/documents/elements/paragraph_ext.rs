//! Fork-only extension builders for [`Paragraph`].
//!
//! Isolated so upstream rebases never conflict here. Pure delegation over the
//! crate's public API. See `FORK_CHANGES.md`.

use super::*;

impl Paragraph {
    /// Sets the paragraph borders (`<w:pPr><w:pBdr/>`).
    ///
    /// Convenience delegator so callers never mutate the public `property`
    /// field directly.
    pub fn set_borders(mut self, borders: ParagraphBorders) -> Self {
        self.property = self.property.set_borders(borders);
        self
    }
}

