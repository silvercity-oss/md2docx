# Stability and support policy

md2docx follows [Semantic Versioning](https://semver.org/). This page says what that covers, so
that a service or a script can depend on it.

## What is stable

From 1.0.0 these do not change incompatibly within a major version:

| Interface | Promise |
| --- | --- |
| The Rust API documented on docs.rs | Source compatible. `Config`, the option structs, the enums and the error and diagnostic types are `#[non_exhaustive]`, so adding a field, a variant or a `with_*` method is a minor release. Extension traits (`FenceRenderer`, `ImageResolver`, `LinkResolver`, `Highlighter`) only ever gain methods that have a default body. |
| Diagnostic codes (`Code`, `docs/diagnostics.md`) | A code never changes meaning or is reused. New codes may appear, so match with a fallback. |
| CLI exit status (`CLI.md`) | The numbers and their meanings. |
| CLI JSON report (`--format json`) | One line per input with `"schema": 1`. Fields may be added within a schema version; a rename, removal or change of meaning is a new schema number. |
| The JSON documents the CLI and library print (`--capabilities`, `--list-codes`, `--print-config`, `Report::to_json`, `Config::settings_json`, the style snapshot of `--extract-reference`) | Each carries a `"schema"` number. Fields may be added within a schema; none is renamed, removed or re-typed. `--print-config`/`settings_json` describe a configuration and are not a file to load back. The style snapshot *is* read back (`--reference-style`): its `preamble_xml`, `styles[].xml` and `theme_xml` fields are the contract. |
| CLI flags and environment variables | A documented flag or `MD2DOCX_*` variable keeps working. New ones may be added. |
| The security defaults | `Config::default()` and `Config::untrusted()` never become more permissive in a minor release. Making them stricter is allowed, and is listed in the changelog. |

## What is not

* **The bytes of the `.docx`.** The same input gives the same document (ids restart for every
  conversion), but the XML a release writes may change to fix a rendering bug or to add fidelity.
  Do not hash the output across versions; compare through the diagnostics and `Stats`.
* **`md2docx::__internals`** and the `internals` cargo feature. They exist for this repository's
  tests and fuzz targets, may change in any release, and are `#[doc(hidden)]`.
* **The exact wording** of error and diagnostic messages. Match on the code or error kind.
* **Anything an external tool prints or draws.** `merman`, `mmdc`, `latex` and `dvipng` are not
  ours; their output can differ between their versions.
* **The `merman` version.** The `mermaid` feature accepts any compatible `0.9.x`; the tests ran
  against the version in `Cargo.lock` (`md2docx::mermaid::MERMAN_VERSION`). No `merman` type is
  part of this crate's API.

## Minimum supported Rust version

* The crate builds on **Rust 1.88** (`rust-version` in `Cargo.toml`, checked by the `MSRV` job
  in CI). The `mermaid` feature needs **Rust 1.95**, merman's own minimum.
* Raising the MSRV is **not** a breaking change; it happens in a minor release, is listed in the
  changelog, and does not require a Rust release newer than six months.

## Deprecation

An item is marked `#[deprecated]` with the version and the replacement for at least one minor
release before a major release may remove it. Removing it earlier is not done.

## Security fixes

Security fixes go into the latest release (see `SECURITY.md`). A fix that has to tighten a
default is released as a patch or minor version, as the changelog notes, even though it can make
a previously accepted document fail to convert.
