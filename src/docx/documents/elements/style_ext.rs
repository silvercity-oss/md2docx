//! Fork-only extension builders for [`Style`].
//!
//! Kept in a dedicated file that upstream `bokuweb/docx-rs` never edits, so
//! rebasing onto new releases produces zero conflicts here. Every method is a
//! pure delegation over the crate's own public API. See `FORK_CHANGES.md`.

use super::*;

impl Style {
    /// Applies shading to the style's run properties (`<w:rPr><w:shd/>`).
    ///
    /// Convenience delegator so callers never mutate the public
    /// `run_property` field directly.
    ///
    pub fn shading(mut self, shading: Shading) -> Self {
        self.run_property = self.run_property.shading(shading);
        self
    }
}

