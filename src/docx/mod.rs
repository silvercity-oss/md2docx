//! A first-party fork of the writer half of [docx-rs](https://github.com/bokuweb/docx-rs)
//! (MIT, Copyright (c) bokuweb; see LICENSE). Reading, JSON/serde, WebAssembly and test
//! code were removed; this crate maintains the rest and changes it where the converter needs.
#![allow(dead_code, unused_imports, unreachable_pub, clippy::all, missing_docs)]

mod documents;
mod errors;
mod escape;
pub(crate) mod macros;
mod types;
pub(crate) mod xml;
mod xml_builder;
mod zipper;

pub(crate) use documents::ids::IdScope;
pub use documents::*;
pub use errors::*;
pub use types::*;
