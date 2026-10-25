# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0]

First release.

### Added
- Markdown to `.docx` conversion: headings, emphasis, links, block quotes,
  nested and task lists, tables, fenced code, rules, local images, YAML front
  matter (skipped).
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
