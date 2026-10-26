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
- `md2docx-cli` command-line tool (`clap`).
- Strict error type (`MissingExternalCli`, `InvalidMarkdown`,
  `DocxGeneration`, `ExternalToolFailed`, `Io`); temp files are removed
  on every path.

### Library
- `markdown_to_docx_writer` (stream to a file); `Config` presets and `with_*`
  setters; `Config`, `Limits`, `GraphOptions`, `MathOptions`, `DocumentOptions`
  and `CrateError` are `#[non_exhaustive]`.
- `DocumentOptions`: page size (A4, Letter, Legal, custom), margins, body and
  code fonts, body size, heading colour; validated up front (`InvalidConfig`).
- `cli` feature (default): library users can use `default-features = false`
  to skip `clap`/`shlex`.
- `markdown_file_to_docx` applies the input limit before reading and refuses
  devices and directories.
- `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`, runnable crate examples.

### Navigation and document features
- Internal links: `[text](#heading)` becomes a hyperlink to a bookmark on the heading
  (GitHub-style slugs, `{#id}`, repeated headings `-1`, `-2`).
- Footnotes show their number in front of the note (the upstream writer wrote none) and the reference
  mark is a real superscript character style.
- `DocumentOptions::{title, author, page_numbers}`; title and author fall back to the
  front matter; the file's created/modified times are the real time.
- Code blocks have no gap between lines, a gap around the block and a thin frame
  (`Theme::border`), so consecutive blocks are separate boxes and a white theme is visible.
- `GraphOptions::on_failure` / `DiagramFailure::KeepAsCode` / `md2docx-cli --keep-failed-diagrams`;
  `CrateError::is_input_error()`; `CommandSpec` is `#[non_exhaustive]` (use `CommandSpec::new`).

### Extension points
- **Syntax highlighting** (`Config::with_highlighting`, `Highlighting`, `Theme`,
  `TokenClass`, `Highlighter`, `BuiltinHighlighter`; `md2docx-cli --highlight`): built-in
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
  one paragraph with line breaks, easy to select and copy; `md2docx-cli --code-blocks single`).
- `Config::compression_level` / `with_compression_level` / `md2docx-cli --compression-level`
  (0 to 9, default **6**): the XML parts are deflated while the archive is written, in one
  pass (about 10x smaller than uncompressed; pictures stay stored).
- `Limits::max_output_bytes` (the finished file, enforced exactly while writing:
  256 MiB; **50 MiB** in `Config::untrusted()`) and `Limits::max_uncompressed_bytes`
  (the size readers must parse, checked before packaging: 1 GiB; 64 MiB untrusted).
  A small input can expand about 40x into XML.

### Diagrams for untrusted input
- `Config::untrusted_with_diagrams()` / `md2docx-cli --untrusted --diagrams`: Mermaid
  rendered by `merman-cli` (no browser) with a locked-down command (stock merman
  line only, `--input-kind mermaid`), a scrubbed environment, at most 20
  diagrams of 16 KiB, 20 s each and 60 s in total; `dolphin` fences stay code.
  `Limits::max_diagrams` (default 100) applies to every configuration.
- Verified against merman-cli 0.9.2 that hostile diagrams (file/URL image
  references in labels, `@{ img }` nodes, CSS `url()`, `themeCSS` `@import`,
  raw SVG as the diagram) neither read local files nor use the network, with
  positive controls; the tests run on every push and weekly against the latest
  merman-cli.

### Own writer, safer defaults, and what an embedding service needs
- **The `.docx` writer is first-party.** The `docx-rs` dependency (last release months old,
  pinned to `zip` 8 with open advisories) is replaced by `src/docx/`, a trimmed fork of its
  writing half (reader, serde, JSON, wasm and about a quarter of the remaining code removed;
  MIT attribution in LICENSE and THIRD_PARTY.md). `zip` is now 9, which clears the three
  `cargo audit` exceptions that were needed. Output changes: identifiers are per conversion
  (equal input gives equal parts), every drawing has its own `wp:docPr` id and accessible
  description, relationship order is deterministic, archive entries are never executable, and
  archives are written in one pass at the configured level.
- **`Config::default()` is the untrusted-input policy.** `Config::trusted()` is the permissive
  preset; `Limits::default()` is the tight set and `Limits::trusted()` the generous one; `MathMode` defaults to `NativeOmml` and
  `GraphOptions::enabled` to `false`. `Config::native_math()` is documented as a trusted preset.
  The `md2docx-cli` binary starts from `Config::trusted()`.
- **Findings and strict mode.** `convert`, `convert_to_writer`, `convert_file` and `analyze` return
  `Diagnostic`s (23 stable `Code`s, `Severity`, `SourceSpan` in the original text) and `Stats`;
  `Config::loss_policy` (`LossPolicy::Allow` default, `Reject` = `CrateError::LossyContent`);
  `md2docx-cli --strict`, with `file:line:column` output. See docs/diagnostics.md.
- **Host-supplied images.** `ImageResolver` / `AssetMap` / `Config::with_image_resolver`: exact
  bytes by key, never a path or URL. Every image is decoded in full and re-encoded as a clean
  PNG under new limits (`max_distinct_images`, `max_image_pixels`, `max_image_dimension`,
  `max_total_decode_pixels`, `max_total_normalized_bytes`, `max_decode_alloc_bytes`); exact
  repeats are cached by SHA-256, different encodings of the same pixels each pay for the decode.
  Alt text, titles, and diagram/formula descriptions go to `wp:docPr`; fence `alt="…"`/`title="…"`.
- **Host control of temporary files and time.** `Config::scratch_dir`, `sweep_scratch`,
  `CancelToken`, `Limits::max_conversion_time`, `RenderContext` for `FenceRenderer::render_with`,
  `CrateError::{Cancelled, TimedOut}`; renderer results stay in memory; `md2docx-cli --scratch-dir`,
  `--deadline`.
- **In-process Mermaid** (`mermaid` feature, Rust 1.95): `Config::with_in_process_mermaid()`,
  merman pinned to 0.9.2.
- **Cleanup.** A tool's whole process group is now ended on *every* exit after it starts
  (including a post-exit workspace-limit or console-limit error, which used to return first),
  by an owned guard; the console cap is rechecked after exit.
- **Footnote content.** Footnotes keep links, pictures, diagrams, native equations and tables
  (their relationships live in the footnotes part). A footnote cannot contain a footnote.
- **Layout and conveniences, all opt-in.** `DocumentOptions::table_of_contents` (static, linked,
  no page numbers; `--toc`), `DocumentOptions::number_headings` (Word multilevel numbering;
  `--number-headings`), `Config::autolink_bare_urls` (`--autolink`),
  `Config::superscript_subscript` (`--sup-sub`), `Config::with_reference_docx` (styles and theme
  from a `.docx`; `--reference-doc`). Always on: table columns sized by content, HTML `id=` /
  `<a name>` anchors usable as `#` link targets, `mailto:` links limited to `subject`, `body`,
  `cc` and `bcc`.
- **A command line for automation.** `md2docx-cli` reads standard input and writes standard output
  (`-`), converts several files (`-o DIR`), validates without writing (`--check`), reports one JSON
  object per input (`--format json`, schema 1), and exits with a documented status that says who has
  to act (0 ok, 2 usage, 3 file I/O, 4 document rejected, 5 `--strict` refusal, 6 external tool, 7
  deadline; docs/cli.md). New flags: `--quiet`, `--base-dir`, the `--max-*` limits (which can only
  be lowered under `--untrusted`), `--link-schemes`, `--no-html-links`, `--title`, `--author`,
  `--page-size`, `--landscape`, `--margin`, `--body-font`, `--body-size`, `--code-font`,
  `--page-numbers`, `--mermaid-engine inprocess` (with the `mermaid` feature),
  `--source-date-epoch`, `--completions`, `--man`; operational options can come from `MD2DOCX_*`
  environment variables. `docs/containers.md` and a `Dockerfile` describe a locked-down worker.
- **Reproducible output.** `DocumentOptions::timestamp` / `Config::with_timestamp` fix the file's
  created and modified time; `SOURCE_DATE_EPOCH` is honoured by the CLI.
- **Links to files and folders.** For documents you trust, relative links can become working
  hyperlinks: `--allow-links local|all`, written relative to the saved `.docx` or as `file:///`
  addresses (`--link-style`), as web addresses (`--link-base`, `--link-tree-base`, `--link-map`),
  and between documents (`--markdown-links docx|follow`, `--link-layout`, `--copy-linked`:
  `file.md#heading` opens the converted document at that heading, and a whole tree of linked
  Markdown files can be converted and copied together). Broken links and missing `#heading`s are
  findings (`link.target-missing`, `link.outside-root`, `link.blocked`, `link.anchor-missing`), so
  `--check --strict` doubles as a link checker. Library: `LinkOptions`, `LocalLinks`, `LinkStyle`,
  `DocumentLayout`, `LinkResolver`, `Report::local_links`. Off by default; docs/links.md.
- **Streams.** `convert_reader`, `analyze_reader` and `analyze_file`.
- **Review fixes.** Footnotes: every note shows its number (also with a table or a line break),
  lists inside them are numbered and declared, they are built in linear time and honour the
  deadline. List markers survive in HTML table cells and when an item starts with a code block,
  quote, table, rule or nested list; HTML tables nest. Heading anchors leave out picture alt text
  and use a formula's source. A bare `\r` ends a line. Link targets keep spaces (as `%20`);
  `link.modified` reports a shortened `mailto:` link, `image.unreadable` a missing local picture
  (the document is still produced). `footnote.undefined` is now information. Common named HTML
  entities (`&copy;`, `&mdash;`, ...) are decoded. Property elements are written in schema order.
  `CrateError::MissingExternalCli` and `CrateError::DocxGeneration` follow the Rust naming rules.
- A display equation that is alone in a list item, quote or task item keeps the paragraph's
  numbering, indent and checkbox (it was replaced by a bare paragraph).
- Docs: docs/embedding.md, docs/diagnostics.md, docs/integration-test-plan.md,
  `examples/embed.rs`, `examples/fixtures.rs`; LibreOffice open/save/reopen test.

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
  HTML table keeps both; `md2docx-cli notes.md` finds images next to a bare file name;
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
- `Config::untrusted()` / `md2docx-cli --untrusted`: native math, diagram fences left
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
