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

### Navigation and document features
- Internal links: `[text](#heading)` becomes a hyperlink to a bookmark on the heading
  (GitHub-style slugs, `{#id}`, repeated headings `-1`, `-2`).
- Footnotes show their number in front of the note (docx-rs wrote none) and the reference
  mark is a real superscript character style.
- `DocumentOptions::{title, author, page_numbers}`; title and author fall back to the
  front matter; the file's created/modified times are the real time (they were 1970).
- Code blocks have no gap between lines, a gap around the block and a thin frame
  (`Theme::border`), so consecutive blocks are separate boxes and a white theme is visible.
- `GraphOptions::on_failure` / `DiagramFailure::KeepAsCode` / `md2docx --keep-failed-diagrams`;
  `CrateError::is_input_error()`; `CommandSpec` is `#[non_exhaustive]` (use `CommandSpec::new`).

### Extension points
- **Syntax highlighting** (`Config::with_highlighting`, `Highlighting`, `Theme`,
  `TokenClass`, `Highlighter`, `BuiltinHighlighter`; `md2docx --highlight`): built-in
  lexers for Rust, Python, JavaScript, TypeScript, Java, C, C++, C#, Go, Kotlin, Swift,
  Ruby, PHP, SQL, Bash, JSON, YAML, TOML, HTML/XML, CSS and diff (linear time, no
  dependencies), VS Code Light+/Dark+ themes, custom themes, and custom highlighters per
  language or as a fallback (spans are validated, panics are caught). `docs/highlighting.md`
  shows `syntect` and `tree-sitter` integrations. `Limits::max_highlight_bytes`.
- `Config::with_fence_renderer(lang, renderer)`: render fenced blocks with your own
  code (`FenceRenderer` trait or a closure) to a PNG; replaces the external command
  when registered for `mermaid`/`dolphin`. Panics are caught, output is checked like
  any image, and the blocks count against the diagram limits.
- `Config::with_html_tag(name, TagStyle)`: style custom or built-in HTML tags
  (inline formatting, colour, highlight, or a block with fill, bar, indent, label).
  Attributes in the document are ignored. Tag names may contain `-`.
- `DocumentOptions::{code_background, link_color, quote_bar_color}` and
  `DocumentOptions::code_blocks` (`CodeBlockLayout::SingleParagraph`: a code block as
  one paragraph with line breaks, easy to select and copy; `md2docx --code-blocks single`).
- `Config::compression_level` / `with_compression_level` / `md2docx --compression-level`
  (0 to 9, default **6**): docx-rs stores archives uncompressed, so the finished
  file is now rewritten with deflate (about 10x smaller; pictures stay stored).
- `Limits::max_output_bytes` (the finished file, enforced exactly while writing:
  256 MiB; **50 MiB** in `Config::untrusted()`) and `Limits::max_uncompressed_bytes`
  (the size readers must parse, checked before packaging: 1 GiB; 64 MiB untrusted).
  A small input can expand about 40x into XML.

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

### Tests
- All 652 CommonMark specification examples are converted in both the default and
  untrusted configurations (well-formed XML, no script-scheme link targets, no
  text dropped); every pulldown-cmark tag and event is accounted for, and the
  test stops compiling if an upgrade adds one; pathological shapes (deep
  nesting, wide and tall tables, huge lines, delimiter runs, quadratic-parsing
  traps, runaway HTML) finish within a time budget on a default thread stack.

### Security
- Audit fixes: entity decoding and hidden-tag detection were quadratic (200 s of CPU
  from a 2 MiB input); memory while building is now bounded by `Limits::max_elements`
  (60 000 in `Config::untrusted()`; a 2 MiB input used to take up to 7.7 GiB); links in
  footnotes no longer leave a dangling relationship id; a Markdown table inside an
  HTML table keeps both; `md2docx notes.md` finds images next to a bare file name;
  integer overflow in code-block indents; list start numbers fit 32 bits; HTML
  attribute lookup ignores text inside quoted values; a fence's picture is no
  longer glued to the next line; image paths outside the base directory are refused
  from the text alone (no existence oracle, no UNC lookups); JPEG/GIF are charged at
  their re-encoded size; the CLI never overwrites its input through links or another
  spelling, writes atomically, and caps `--timeout` under `--untrusted --diagrams`;
  tool supervision counts files at any depth, kills helpers after success too and
  refuses a symlinked output; a refused document writes nothing to a caller's writer.
- `preprocess`, `Asset*`, `Preprocessed` and the renderer plumbing are no longer public
  (they remain reachable under `#[doc(hidden)] __internals` for this crate's tests);
  the public enums are `#[non_exhaustive]`; `OmmlError` has a private field and `message()`.
- `Limits::max_uncompressed_bytes` is 64 MiB in `Config::untrusted()`.
- Code blocks in `CodeBlockLayout::SingleParagraph` are split into paragraphs of at most
  500 lines (LibreOffice's layout time grows with the square of a paragraph's lines).
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
- Content between the tags of an HTML `<table>` (an indented block or text outside
  any cell) was silently dropped; it now appears in front of the table, as browsers do.
- `![a ![b](x)](y)` lost the outer alt text.
- Tabs in code blocks are real Word tab elements (a raw tab inside text is not
  reliably shown), and adjacent text events are merged into one run, which shrinks
  output for input full of escapes or stray delimiters.
- `---` / `+++` blocks in the middle of a document were silently deleted as
  metadata; front matter is now recognised only at the start.
- `<me@example.com>` autolinks lost their link target.
