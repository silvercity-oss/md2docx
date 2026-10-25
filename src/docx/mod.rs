//! A first-party fork of the writer half of [docx-rs](https://github.com/bokuweb/docx-rs)
//! (MIT, Copyright (c) bokuweb; see LICENSE). Reading, JSON/serde, WebAssembly and test
//! code were removed; this crate maintains the rest and changes it where the converter needs.
#![allow(unused_imports, unreachable_pub, clippy::all, missing_docs)]

pub(crate) mod macros;
mod documents;
mod errors;
mod escape;
mod types;
pub(crate) mod xml;
mod xml_builder;
mod zipper;

pub use documents::*;
pub use errors::*;
pub use types::*;
