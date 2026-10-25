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
  commands and embedded as pictures.
- LaTeX math in two modes: native, editable OMML equations (no external tool),
  or transparent PNGs via `latex` + `dvipng` or a custom command.
- Dependency-free LaTeX to OMML converter.
- `md2docx` command-line tool (`clap`).
- Strict error type (`MissingExternalCLI`, `InvalidMarkdown`,
  `DocxGenerationError`, `ExternalToolFailed`, `Io`); temp files are removed
  on every path.

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
- Tool supervision: console and disk caps, process-group kill on Unix.
- OMML parser: nesting limit and no-progress guard (fixed an infinite loop in
  `\sqrt[...]` and stack overflow on deeply nested input).
- Control characters stripped so output is always valid XML.

### Fixed
- `---` / `+++` blocks in the middle of a document were silently deleted as
  metadata; front matter is now recognised only at the start.
- `<me@example.com>` autolinks lost their link target.
