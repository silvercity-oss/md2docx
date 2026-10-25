# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0]

First release.

### Added
- Markdown to `.docx` conversion: headings, emphasis, links, block quotes,
  nested and task lists, tables, fenced code, rules, local images, footnotes
  (native Word footnotes), definition lists, GitHub alerts, raw HTML (common
  inline and block tags), YAML/TOML front matter (skipped).
- `mermaid` and `dolphin` fences rendered to PNG by configurable external
  commands and embedded as pictures. Mermaid defaults to `merman-cli` (Rust, no
  browser); `--mermaid-engine mmdc` / `MermaidEngine::Mmdc` selects the official CLI.
- LaTeX math in two modes: native, editable OMML equations (no external tool),
  or transparent PNGs via `latex` + `dvipng` or a custom command.
- Dependency-free LaTeX to OMML converter.
- `md2docx` command-line tool (`clap`).
- Strict error type (`MissingExternalCLI`, `InvalidMarkdown`,
  `DocxGenerationError`, `ExternalToolFailed`, `Io`); temp files are removed
  on every path.

### Library
- `markdown_to_docx_writer` (stream to a file); `Config` presets and `with_*`
  setters; `Config`, `Limits`, `GraphOptions`, `MathOptions`, `DocumentOptions`
  and `CrateError` are `#[non_exhaustive]`.
- `DocumentOptions`: page size (A4, Letter, Legal, custom), margins, body and
  code fonts, body size, heading colour; validated up front (`InvalidConfig`).
- `cli` feature (default): library users can use `default-features = false`
  to skip `clap`/`shlex`. docx-rs's unused `image` feature is off (no TIFF/BMP).
- `markdown_file_to_docx` applies the input limit before reading and refuses
  devices and directories.
- `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`, runnable crate examples.

### Diagrams for untrusted input
- `Config::untrusted_with_diagrams()` / `md2docx --untrusted --diagrams`: Mermaid
  rendered by `merman-cli` (no browser) with a locked-down command (stock merman
  line only, `--input-kind mermaid`), a scrubbed environment, at most 20
  diagrams of 16 KiB, 20 s each and 60 s in total; `dolphin` fences stay code.
  `Limits::max_diagrams` (default 100) applies to every configuration.
- Verified against merman-cli 0.9.2 that hostile diagrams (file/URL image
  references in labels, `@{ img }` nodes, CSS `url()`, `themeCSS` `@import`,
  raw SVG as the diagram) neither read local files nor use the network, with
  positive controls; the tests run on every push and weekly against the latest
  merman-cli.

### Raw HTML
- `<a href>` hyperlinks (safe schemes only; off in `--untrusted`), alignment via
  `align`/`text-align`/`<center>`, real lists (`<ol start>`), tables, `<pre>`,
  `<blockquote>`, and `<img width>`.

### Security
- Image mode runs a local TeX engine on document content; use native mode for
  untrusted input. See the README.
- `Config::untrusted()` / `md2docx --untrusted`: native math, diagram fences left
  as code, no local files, tight limits, no subprocess.
- TeX hardening: denylist for file/shell/catcode primitives and `^^`, paranoid
  file-access environment (`openin_any=p`, `openout_any=p`, `shell_escape=f`).
- Hyperlinks restricted to `http`, `https`, `mailto` (configurable); image access
  policy (`ImageAccess`), regular-file and size checks, decompression-bomb guard.
- `Limits` for input size, block counts, formula/diagram size, image and
  workspace bytes, footnote expansion.
- Tool supervision: console and disk caps (also enforced after the tool exits),
  a total tool-time budget per document, process-group kill on Unix.
- OMML parser: nesting limit and no-progress guard (fixed an infinite loop in
  `\sqrt[...]` and stack overflow on deeply nested input).
- Control characters stripped so output is always valid XML.

### Fixed
- `---` / `+++` blocks in the middle of a document were silently deleted as
  metadata; front matter is now recognised only at the start.
- `<me@example.com>` autolinks lost their link target.
