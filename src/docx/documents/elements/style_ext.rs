//! Fork-only extension builders for [`Style`].
//!
//! Kept in a dedicated file that upstream `bokuweb/docx-rs` never edits, so
//! rebasing onto new releases produces zero conflicts here. Every method is a
//! pure delegation over the crate's own public API. See `FORK_CHANGES.md`.

use super::*;

impl Style {}
