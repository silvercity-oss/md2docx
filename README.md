# silvercity-oss

A pure-Rust library (plus the `md2docx` CLI) that converts Markdown into
high-fidelity Word `.docx` files, including **LaTeX math** and **Mermaid /
Dolphin diagrams** that plain Word writers cannot express natively.

```rust
use silvercity_oss::{markdown_to_docx, Config};

let docx: Vec<u8> = markdown_to_docx("# Hi\n\n$$E = mc^2$$", &Config::default())?;
std::fs::write("out.docx", docx)?;
```

## Using it as a library

```toml
[dependencies]
# default-features = false: skip the `md2docx` binary's dependencies (clap, shlex).
silvercity-oss = { version = "0.1", default-features = false }
```

```rust
use silvercity_oss::{markdown_to_docx, Config, CrateError};

// `Config::default()` is the untrusted-input policy: native math, no TeX or diagram process,
// no local files, no raw-HTML links, tight limits. TeX, external renderers and local files
// are opt-in through `Config::trusted()` or explicit settings.
let docx = markdown_to_docx(&user_markdown, &Config::default())?;
```

| Function | Use |
| --- | --- |
| `markdown_to_docx(&str, &Config) -> Result<Vec<u8>>` | The common case |
| `markdown_to_docx_writer(&str, &Config, impl Write + Seek)` | Write to a `File` or `Cursor`. The archive is built in memory first and written in one piece, so a refused or failed conversion writes nothing |
| `markdown_file_to_docx(&Path, &Config)` | Reads the file with the input-size limit applied *before* it is read (devices and directories are refused); relative images resolve against its directory |
| `convert(&str, &Config) -> Result<Conversion>` | The document **and** what could not be carried over: `diagnostics` (stable code, severity, span in your text) and `stats` |
| `convert_to_writer`, `convert_file` | The same for a writer or a file |
| `analyze(&str, &Config) -> Result<Report>` | The findings and statistics without building the archive: validate or preview before converting |
| `sweep_scratch(&Path, Duration)` | Remove scratch directories left by a worker that was killed from outside |
| `omml::latex_to_omml(&str)` | The LaTeX to OMML converter on its own |

Guides: [embedding it in a service](docs/embedding.md) (trust model, recipe, what needs host
isolation), [diagnostics reference](docs/diagnostics.md), [integration test plan](docs/integration-test-plan.md),
and the runnable `examples/embed.rs`.

**Configuration.** `Config` is `#[non_exhaustive]`, so new options never break your
code: start from a preset, then chain `with_*` methods or assign fields.

| Preset | For |
| --- | --- |
| `Config::default()` = `Config::untrusted()` | **The safe default**, for user uploads, web forms, services. Native math, diagrams off, no local images, no `<a>` links, tight limits. **Starts no subprocess and reads no file** |
| `Config::untrusted_with_diagrams()` | The same, plus Mermaid diagrams via `merman-cli` (no browser). Starts one tightly bounded process per diagram; see "Diagrams from untrusted input" |
| `Config::trusted()` | Your own documents: math as pictures via TeX (the original spec's default), diagrams via `merman-cli`, images from `base_dir`, generous limits. **Runs external programs and reads files** |
| `Config::native_math()` | `trusted()` with native equations, so no TeX needed. **Not an untrusted preset**: removing TeX does not remove local-file access, diagram processes or raw HTML links |

TeX, external renderers and local files are therefore behind explicit, trusted
configuration. Everything on `Config` (and any `FenceRenderer`, highlighter or
asset resolver) is chosen by the embedding program, never by the document.

`DocumentOptions` sets the page (`A4`, `Letter`, `Legal`, or `Custom` twips for
landscape or any size), margins, body and code fonts, body size and heading
colour. All options are validated up front: out-of-range values are a
`CrateError::InvalidConfig` naming the option, before any work is done.

### Findings and strict conversion

Not everything in Markdown has a faithful Word equivalent, and an embedder that stores or
delivers the result usually wants to know. Every conversion records what it dropped or degraded
as `Diagnostic`s: `image.remote`, `link.dropped`, `footnote.unreferenced`, `list.depth-clamped`,
`diagram.not-rendered`, `math.unknown-command`, ... (all codes in [docs/diagnostics.md](docs/diagnostics.md)).
Each has a severity and a span that indexes the string you passed in.

```rust
use silvercity_oss::{convert, Config, CrateError, LossPolicy};

let strict = Config::default().with_loss_policy(LossPolicy::Reject);
match convert(&markdown, &strict) {
    Ok(done) => store(done.docx),
    Err(CrateError::LossyContent { diagnostics }) => show_author(&diagnostics), // line:column, code, message
    Err(other) => return Err(other.into()),
}
```

`LossPolicy::Allow` (the default, and the behaviour of every earlier version) returns the
degraded document and still reports the findings through `convert`; the byte-returning
functions then return success without them, so choose the policy explicitly. `md2docx --strict`
does the same on the command line, printing `file:line:column: warning[code]: message`.

### Images from your own storage

`Config::with_image_resolver(AssetMap)` (or any `ImageResolver`) gives the converter the exact
bytes for keys the document names, without enabling file paths or the network. Every image,
whatever its source, is decoded in full, re-encoded as a clean PNG and charged to configurable
limits (source bytes, distinct images, pixels, aggregate decode work, aggregate normalised
bytes). Alt text and titles are written to the picture's accessibility properties. See
[docs/embedding.md](docs/embedding.md#images).

### Scratch storage, cancellation and deadlines

`Config::scratch_dir` makes every temporary artifact live under a directory you own (no
environment variable is touched); `CancelToken` and `Limits::max_conversion_time` stop a
conversion cooperatively, killing a running tool's process tree first. Results from a
`FenceRenderer` or `ImageResolver` stay in memory. These are library-level checks: a worker
that must survive hostile input still needs a memory limit and an outside supervisor
(see the table in [docs/embedding.md](docs/embedding.md#what-the-library-checks-and-what-needs-the-process-around-it)).

### Extending it

Both extension points are set on the `Config` by *you*, never by the Markdown
author, so they are safe with `Config::untrusted()` and the other presets.

```rust
use silvercity_oss::{markdown_to_docx, BlockStyle, Config, FenceError, TagStyle};

let cfg = Config::untrusted()
    // ```chart fences become pictures from your own code (a closure or a `FenceRenderer`).
    .with_fence_renderer("chart", |source: &str| -> Result<Vec<u8>, FenceError> {
        Ok(draw_chart_png(source)) // PNG bytes
    })
    // <mark>..</mark> highlights; <callout>..</callout> is a shaded block with a label.
    .with_html_tag("mark", TagStyle::new().highlight("FFFF00"))
    .with_html_tag(
        "callout",
        TagStyle::new().block(BlockStyle::new().fill("FFF4CE").bar_color("E0A800").label("Note: ")),
    );
```

* **Fence renderers** (`Config::with_fence_renderer`) take the text of a fenced
  block and return PNG bytes. The result is held to the same size, pixel and
  format checks as any image. Registering `mermaid` or `dolphin` replaces the
  external command for that language, which is how to plug in an in-process
  renderer. A renderer cannot be interrupted, so one that may run long on
  hostile input must bound itself; a panic is caught and reported as an error.
* **Tag styles** (`Config::with_html_tag`) map an HTML tag, including your own
  such as `<call-out>`, to bold/italic/underline/strike/monospace, a text colour
  or highlight, or to a block with a fill, a left bar, an indent and a bold
  label. Attributes in the document are ignored, so the author can only switch
  styles *you* defined on and off. Names and colours are validated up front.
* **Syntax highlighting** (`Config::with_highlighting(Highlighting::new(Theme::vscode_dark()))`,
  `md2docx --highlight light|dark`): built-in lexers for 21 languages and the VS Code
  Light+/Dark+ themes, or your own `Highlighter` (a closure works) for anything else. How
  to plug in `syntect` or `tree-sitter` for better accuracy, with tested code, is in
  [docs/highlighting.md](docs/highlighting.md).
* **Errors by cause.** `CrateError::is_input_error()` is true when the document itself is at
  fault (malformed, over a limit, forbidden): show that to the author. Missing tools, bad
  configuration and I/O are the operator's. A renderer that rejects a diagram is
  `ExternalToolFailed`, which can be either; set `GraphOptions::on_failure =
  DiagramFailure::KeepAsCode` (`--keep-failed-diagrams`) to keep such blocks as code instead of
  failing, which is usually what a service wants for other people's documents.
* **Document properties and page numbers.** `DocumentOptions::{title, author, page_numbers}`;
  `title:` and `author:` in the front matter are used when the options are unset.
* `DocumentOptions` also sets `code_background`, `link_color` and
  `quote_bar_color`.
* `Config::with_compression_level(0..=9)` sets how the finished `.docx` is
  compressed (default 6; the underlying writer stores everything uncompressed, which made
  files about 10x larger). Pictures stay stored; they are compressed already.

Not offered, on purpose: handing out the Markdown parser's options or raw
internal document-model paragraphs. Every event has to pass the converter's safety checks, and
exposing the internal model would freeze it as public API.

**Good to know.**

* **Blocking.** Conversion is synchronous and may wait on external tools. From
  async code use `spawn_blocking`.
* **Thread-safe.** `Config` is `Send + Sync`. Each call has its own scratch
  directory, removed when it returns (also on error); concurrent calls are
  independent (tested with 16 threads).
* **Deterministic identifiers.** Relationship, footnote, picture and bookmark ids restart
  for every conversion (they are per thread and reset by each call), so the same input
  yields the same parts apart from the document's created/modified timestamps. No state
  leaks between conversions in a long-lived process.
* **Errors** are `CrateError` (`#[non_exhaustive]`, `std::error::Error + Send +
  Sync`): `MissingExternalCLI`, `ExternalToolFailed`, `InvalidMarkdown`,
  `InvalidConfig`, `LimitExceeded`, `UnsafeContent`, `DocxGenerationError`,
  `Io`. The crate is `#![forbid(unsafe_code)]` and is tested to return errors, not panic, on hostile input (random, spec-corpus and pathological runs); a panic in a `FenceRenderer` you register is caught, though the default panic hook still prints it and `panic = "abort"` builds would still abort.
* **Dependencies.** Library-only builds compile 80 crates (94 with the CLI).
  External tools (TeX, merman-cli, mmdc) are programs you install, never linked.

### Supported Rust versions

`rust-version = "1.88"`, checked in CI. That is the **floor our dependencies
set, not a choice**: `zip` 9 requires 1.88 (older `zip` releases carry published
advisories), so supporting older compilers would mean shipping a vulnerable archive library.
Raising the minimum would buy nothing but fewer users. Policy: the MSRV only
rises when a dependency forces it, in a minor release, noted in the changelog.
Building from source needs only a current rustup toolchain; Debian 13's
packaged Rust (1.85) and Ubuntu 24.04's (1.75) are too old, as they are for
most of the ecosystem. Note this is about *building* this crate; `merman-cli`,
a separate program, needs 1.95 to build (`cargo install merman-cli`). The optional
`mermaid` feature (in-process diagrams through the `merman` crate, see "Diagrams") has the
same 1.95 requirement; it is tested separately in CI and does not raise the MSRV of the rest.

## Pipeline

```
Markdown ──► 1. preprocess ──► 2. pulldown-cmark events ──► 3. docx writer ──► .docx bytes
             (render, rewrite)                              (+ OMML injection)
```

| Module | Role |
| --- | --- |
| `preprocess.rs` | Finds `mermaid`/`dolphin` fences and `$`/`$$` math with `pulldown-cmark` (`ENABLE_MATH`, so code spans, escapes and nested quotes/lists are handled by the real parser), renders them, and rewrites each into `![alt](dest)`. |
| `renderer.rs` | Runs external CLIs under an owned process-tree guard (timeout, console and disk caps, cancellation), names assets by SHA-256, owns the RAII `Workspace` (`tempfile::TempDir` under `Config::scratch_dir`), `sweep_scratch`. |
| `images.rs` | The one place images are measured, decoded, re-encoded and charged to the image budgets. |
| `diagnostics.rs`, `control.rs` | Findings, spans mapped back to the original text, `LossPolicy`; cancellation and the conversion deadline. |
| `mermaid.rs` | (feature `mermaid`) the in-process Mermaid `FenceRenderer`. |
| `docx/` | The `.docx` writer: a first-party, trimmed fork of the writing half of [docx-rs](https://github.com/bokuweb/docx-rs) (MIT; see LICENSE), maintained here. |
| `omml.rs` | Dependency-free LaTeX → OMML converter. |
| `parser.rs` | The event loop mapping CommonMark events onto the internal `docx` writer, and the OMML injection step. |

Everything rendered to a file lives in one `TempDir` (under `Config::scratch_dir`) that is
deleted when the conversion returns, on success and on every error path; renderer callbacks
keep their PNG in memory instead.

## Diagrams

`mermaid` fences are rendered to PNG by an external command; you choose which:

| Engine | What it is | Choose it when |
| --- | --- | --- |
| **`merman`** (default) | [`merman-cli`](https://github.com/Latias94/merman), a headless Rust implementation of Mermaid. `cargo install merman-cli` | Almost always: no browser or JavaScript runtime, ~0.1-1 s per diagram, and it enforces its own layout and raster limits |
| `mmdc` | The official [Mermaid CLI](https://github.com/mermaid-js/mermaid-cli); drives a headless Chrome. `npm install -g @mermaid-js/mermaid-cli` | You need exact Mermaid.js output, or a diagram type merman does not support yet |

Switch with `md2docx --mermaid-engine mmdc` or, in code,
`config.graphs.use_engine(MermaidEngine::Mmdc)`. `--mermaid-command` /
`config.graphs.mermaid` replaces the command entirely (any `CommandSpec`
that writes a PNG).

merman is an independent project, not affiliated with Mermaid, and it
re-implements the renderer, so output can differ from `mmdc`. We run it with
`--font-measurement native`: its default text metrics are approximate and
clip labels in some boxes (class and ER diagrams showed "Anima", "boo"), while
measuring with real installed fonts fixed that. That costs about a second per
diagram for font discovery, and the layout depends on the fonts installed on
the machine. We rendered flowchart, sequence, class, state, ER, Gantt, pie and
architecture diagrams; check yours, and fall back to `mmdc` if one does not
look right. If a tool is missing, the error says how to install it or which
switch to use.

`dolphin` fences use the configured Dolphin command. There is no standard
Dolphin CLI; the default `dolphin -i {input} -o {output}` is a guess, so set
`config.graphs.dolphin` for your tool. Commands are `CommandSpec`s: a program
plus arguments with `{input}`, `{output}` and `{dpi}` placeholders. Identical
diagrams are rendered once. On a headless/root host `mmdc` usually needs a
Puppeteer config (`-p puppeteer.json` with `--no-sandbox`); add it to `args`.

### Diagrams without a child process

With the `mermaid` cargo feature (Rust 1.95), `Config::with_in_process_mermaid()` renders Mermaid
inside the process through the pure-Rust [`merman`](https://crates.io/crates/merman) crate
(pinned to the tested version, PNG export and eleven common diagram families only). It is a
`FenceRenderer`, so it spawns nothing, writes nothing to disk, and shares the conversion's
deadline and `CancelToken`; it suits workers that are allowed exactly one process. It measures
text with merman's deterministic metrics, so labels can differ slightly from the `merman-cli`
engine above. See [docs/embedding.md](docs/embedding.md#diagrams-and-a-one-process-worker).

### Diagrams from untrusted input

`Config::untrusted()` leaves diagram fences as code, so it never starts a
process. If your users need diagrams, `Config::untrusted_with_diagrams()`
(CLI: `md2docx --untrusted --diagrams`) renders **Mermaid only**, with
`merman-cli`, under these rules:

* The only accepted command is the stock merman command line (you may change the
  program *path*); anything else, including `mmdc`, is an `InvalidConfig` error.
  `dolphin` fences stay code.
* The tool always gets `--input-kind mermaid`, so diagram text is never taken to
  be an SVG document, and runs in the private scratch directory with a scrubbed
  environment (only `PATH`; `HOME`/`TMPDIR` point at the scratch directory).
* At most 20 diagrams of 16 KiB each, 20 s per diagram, 60 s in total, plus the
  usual console, disk and output-size caps. Oversized layouts are also refused
  by merman itself.

**Local files and the network.** The worry with any renderer is a diagram that
makes it fetch `file:///etc/...` or a URL. We tested merman 0.9.2 with a
magenta marker image and a local web server, across: `<img>` in HTML labels,
the `@{ img: ... }` image-node syntax (file, plain path and URL), CSS `url()` in
`style`/`classDef`, `%%{init}%%` `themeCSS` with `@import`, Markdown-string
labels, `<iframe>`/`<script>`, `click` callbacks, and raw SVG pasted in as the
diagram. **None embedded the file or made a connection.** The controls matter:
given an SVG *as SVG* it still did not load `file://`, plain-path or `http://`
images, while it did render an embedded `data:` image (14,400 marker pixels), so
the detector can see a leak when there is one. In other words merman's
rasterizer accepts only embedded `data:` images, so there is no local-file
fetching to trim out. That is a property of the version tested, not a promise
the crate can make, so these checks are tests (`tests/merman_hardened.rs`): CI
runs them against the pinned version on every push and against the **latest**
merman-cli every week, to catch a release that changes the behaviour.

What remains is what any external program brings: a third-party binary parsing
hostile text (memory-safe Rust, but not audited by us), CPU and memory use
(bounded by the caps above), and supply chain (pin the version you have
tested). If that is still too much, keep `Config::untrusted()` and run the
converter in a container with no network and no sensitive files.

## Math modes (`config.math.mode`)

* **`MathMode::ImageFallback`** (trusted input only): each formula becomes a transparent
  PNG. Default engine is `latex` + `dvipng -T tight -bg Transparent`
  (TeX Live); `MathImageEngine::Command(..)` plugs in anything else (a
  MathJax/headless-browser node script, …) that reads LaTeX from `{input}` and
  writes a PNG to `{output}`; `{display}` is `1`/`0`.
* **`MathMode::NativeOmml`**: no external tools. `omml.rs` converts LaTeX to
  OMML, and `parser.rs` swaps placeholder runs in the rendered
  `document.xml` for `<m:oMath>` / `<m:oMathPara>` before zipping (the writer
  exposes the rendered parts between `build()` and `pack()`). Equations stay
  editable in Word. A paragraph containing only `$$..$$` becomes a centred
  display equation; display math mixed into text is inline.

Supported LaTeX: fractions/binomials, sub/superscripts and primes, roots,
`\sum \int \prod \bigcup …` with limits, `\left…\middle…\right` delimiters,
accents, `\overline`/`\underline`, `\lim`-style limits, matrices
(`matrix pmatrix bmatrix Bmatrix vmatrix Vmatrix array`), `cases`/`aligned`/
`gather`, `\text`, `\mathbf/\mathbb/\mathcal/…`, Greek letters and a large
symbol table. Unknown commands are kept visible as literal text. Unbalanced
`\left`, `\begin`, or missing arguments is an `InvalidMarkdown` error with the
line number. (Formulas with unbalanced `{}` are not recognised as math by
`pulldown-cmark` at all and stay as plain text.) `&` alignment points in
`aligned` collapse to a thin space.

## Markdown coverage

| Input | Result in Word |
| --- | --- |
| Headings (ATX, setext, `{#id .class}` attributes) | Heading 1-6 styles; attributes dropped |
| Emphasis, strong, strikethrough, inline code, hard/soft breaks, escapes, entities | Character formatting |
| Links (inline, reference, autolink `<url>`, `<email>`) | Hyperlinks, **only for `http`, `https`, `mailto`** (see Security); other targets keep their text |
| Block quotes (nested), GitHub alerts `> [!NOTE]` | Indented with a bar; alerts get a bold label line |
| Bullet / ordered / nested / task lists, start numbers | Native Word numbering (each ordered list restarts); `☐`/`☑` for tasks |
| Tables (alignment, inline formatting) | Word tables |
| Fenced, tilde and indented code | Optionally syntax-highlighted (see "Extending it"). By default one monospace shaded paragraph per line (`DocumentOptions::code_blocks`: `CodeBlockLayout::SingleParagraph` makes the whole block one paragraph with line breaks, so a triple-click selects it for copying; `--code-blocks single`), spaces and tabs kept, indented inside lists and quotes. The language after the fence is not shown (`mermaid`, `dolphin` and math fences are handled as described above) |
| Thematic breaks | A rule |
| Footnotes `[^1]` | Native Word footnotes (unknown labels stay literal) |
| Definition lists | Bold term, indented definitions |
| YAML / TOML front matter | Dropped, but only at the very start of the file and only if it looks like metadata. `---` blocks elsewhere are normal content |
| Raw HTML | Rendered the way a browser would show it for the tags people use in READMEs: see the table below. Scripts, styles, comments, iframes, objects and unknown tags contribute at most their text |
| Images | Local PNG/JPEG/GIF (Markdown `![]()` or HTML `<img>` with `width=`) scaled to the text width, subject to the image policy; SVG, WebP and corrupt images, and all remote or `data:` images become `[alt text]` (nothing is fetched) |
| `$..$` / `$$..$$` math, `mermaid` / `dolphin` fences | See above |

Internal links: `[text](#heading-slug)` jumps to that heading (GitHub-style slugs, `{#id}`
respected, repeated headings numbered `-1`, `-2`; a link to a heading that does not exist stays
plain text; links inside footnotes stay text).

Not supported: bare-URL autolinking, wiki links, super/subscript via `^x^`/`~x~` (use
`<sup>`/`<sub>`), tables inside footnotes (dropped), math inside footnotes (shown as source text),
anchors from raw HTML (`<a href="#x">` stays text, `<a name>` is ignored), a table of contents,
reference `.docx` templates, heading numbering and content-based table column widths (columns
are equal). Code blocks can be highlighted, see "Extending it".

### Raw HTML

Markdown files in the wild are full of HTML. These tags are interpreted, in
every mode:

| HTML | Word result |
| --- | --- |
| `<b> <strong> <i> <em> <u> <s> <del> <sup> <sub> <code> <kbd> <br>` | Character formatting and line breaks |
| `<p> <div> <center> <h1>-<h6>` with `align="center\|right\|left\|justify"` or `style="text-align:..."` | Aligned paragraphs and headings (the alignment ends with the element) |
| `<ul> <ol start=N> <li>` (nestable) | Real Word bulleted / numbered lists |
| `<table> <tr> <th> <td>` | A real Word table; `<th>` cells are bold and shaded. Missing close tags and ragged rows are tolerated |
| `<pre>` | Code lines with whitespace and indentation preserved |
| `<blockquote>`, `<hr>`, `<details><summary>` | Quote bar, rule, plain text |
| `<img src=.. alt=.. width=..>` (pixels or `%`) | The same image policy as Markdown images; sized as requested, never wider than the page |
| `<a href=..>` | **Default: a hyperlink**, only for `http`, `https`, `mailto` (also around an `<img>`, for badges). **`--untrusted`: dropped, text kept** |
| `<script> <style> <iframe> <object> <embed> <template> <head> <title>` and comments | Dropped with their content |
| Everything else (`<svg>`, `<form>`, `<span>`, custom tags...) | The tag is dropped, the text kept |

Attributes are never interpreted except those in the table (`align`, `style`'s
`text-align`, `start`, `src`, `alt`, `width`, `href`), so `onclick=`,
`style="background:url(...)"` and the like have no effect. `--untrusted` renders
the same HTML; the only differences are `<a>` links and the image policy.

## CLI

```
md2docx notes.md                                  # -> notes.docx, native equations
md2docx notes.md -o out.docx --math-mode image    # PNG formulas via latex + dvipng
md2docx notes.md --dolphin-command 'dolphin -i {input} -o {output}'
md2docx notes.md --mermaid-engine mmdc          # official Mermaid CLI instead of merman
md2docx notes.md --mermaid-command 'mmdc -i {input} -o {output} -s 2 -p puppeteer.json'
```

| Flag | Meaning |
| --- | --- |
| `<input>` | Markdown file (relative image paths resolve against its directory) |
| `-o, --output` | Output path (default: `<input>.docx`; refuses to overwrite the input) |
| `--math-mode native\|image` | Default **`native`** (editable OMML, no subprocess). `image` needs a TeX install |
| `--untrusted` | **Use this for input you do not control.** Native math, diagram fences stay code, no local images, tighter limits; starts no subprocess. Cannot be combined with switches that would loosen it |
| `--diagrams` | Only with `--untrusted`: also render Mermaid diagrams with `merman-cli` (hardened, bounded; see "Diagrams from untrusted input"). Needs `cargo install merman-cli` |
| `--image-access none\|base-dir\|any` | Default `base-dir`: only images inside the input file's directory |
| `--mermaid-engine merman\|mmdc` | Mermaid renderer; default `merman` (Rust, no browser) |
| `--dolphin-command`, `--mermaid-command` | Command templates, split with shell quoting rules; must contain `{input}` and `{output}` (`{dpi}` optional) |
| `--latex`, `--dvipng` | Executables for `image` math (e.g. `xelatex`) |
| `--timeout <seconds>` | Kill a renderer that runs longer (default 60) |
| `--code-blocks <lines\|single>` | Code block layout: a paragraph per line (default) or one paragraph per block, easy to select and copy |
| `--compression-level <0-9>` | Deflate level of the `.docx`: 0 stores it, 9 is smallest (default 6) |
| `--highlight none\|light\|dark` | Syntax-highlight code blocks (VS Code Light+/Dark+ colours; default `none`) |
| `--keep-failed-diagrams` | A `mermaid`/`dolphin` block that cannot be rendered (tool missing, invalid diagram) stays a code block instead of failing the conversion |

The CLI converts files you point it at, so it starts from `Config::trusted()` (with
`native` math unless you pass `--math-mode image`); `--untrusted` switches to the safe
preset. On Windows,
npm-installed tools are `.cmd` shims, so pass `mmdc.cmd` explicitly
through `--mermaid-command`.

## Security

> **Security Advisory Notice: Image Mode runs a local TeX engine on document content.**
>
> `MathMode::ImageFallback` (CLI: `--math-mode image`) invokes `latex` on a
> document generated from the formulas in the Markdown. `-no-shell-escape`
> blocks `\write18` command execution, **but it does not stop `\input` from
> reading host files**. We measured this: with only `-no-shell-escape`,
> `$\input{/path/to/secret.txt}$` typeset the file's contents into the formula
> image, which then ended up in the `.docx`. TeX macros such as `\input`,
> `\include`, `\openin`/`\read` are the risk.
>
> **Native OMML Mode must be mandated whenever the Markdown comes from an
> untrusted source** (user uploads, web forms, third-party feeds, multi-tenant
> servers). It converts LaTeX to OMML in-process, never spawns `latex`, and
> cannot read host files. In library code use `Config::default()` (the same as
> `Config::untrusted()`); `Config::native_math()` alone does **not** make input safe, it only
> removes TeX (see "What is not covered"). With the CLI use `md2docx --untrusted`.
> Image mode is for Markdown you wrote yourself.

Image mode is nevertheless hardened, in layers, so a mistake in one does not
expose the host: (1) a denylist refuses formulas containing file, shell, package
or catcode primitives (`\input`, `\write`, `\openin`, `\csname`, `\catcode`,
`\special`, `\pdf*`, `^^` notation, ...) before `latex` starts; (2) TeX runs in
paranoid file-access mode (`openin_any=p`, `openout_any=p`, `shell_escape=f`), so
even a bypass can only touch its scratch directory; we test this against a real
TeX install; (3) the scratch directory is private, size-capped and deleted.

### What is defended, and how

| Threat in the Markdown | Gate |
| --- | --- |
| LaTeX that reads/writes host files or runs programs | Native mode never starts TeX; image mode has the layers above |
| Deeply nested formulas (stack overflow aborts the whole process) | Nesting limit of 128 levels in the OMML parser, plus a no-progress guard. Found by fuzzing: an infinite loop in `\sqrt[...]` |
| `[x](javascript:...)`, `file:`, UNC `\\host\share`, `ms-excel:` and other handler links | Only `http`, `https`, `mailto` become hyperlinks (configurable via `allowed_link_schemes`); the link text is kept. Whitespace/control tricks like `java<TAB>script:` are normalised first |
| `![x](../../secret.png)`, absolute paths, symlinks out of the tree | `ImageAccess::BaseDir` (default) resolves symlinks and refuses anything outside the base directory; `None` reads nothing; `Any` is opt-in |
| `![x](/dev/zero)`, FIFOs, devices | Files are inspected before opening and must be regular, size-limited files |
| Image decompression bombs | Pixel-count check on the header before any decode, decoder allocation ceiling, per-file and total byte budgets |
| Control characters in text (make Word call the file corrupt) | Stripped from all input; the XML is checked well-formed in tests |
| Huge input, many blocks, footnote amplification | `Limits`: input size, number of diagrams/formulas, formula and diagram size, image and workspace bytes, footnote expansion, and `max_elements` (paragraphs, runs, cells and lists built, 60 000 untrusted: building 1M paragraphs peaked above 7 GiB before this limit existed) |
| Small input that expands into a huge `.docx` (about 40x as XML: 1 MB of code lines is 39 MB before compression) | `Limits::max_uncompressed_bytes` (the size readers must parse, checked before packaging: 1 GiB default, 64 MiB untrusted; building memory is bounded by `max_elements`) and `Limits::max_output_bytes` (the finished compressed file, enforced exactly while it is written: 256 MiB default, **50 MiB in `Config::untrusted()`**) |
| A renderer that hangs, floods output, fills the disk or leaves helpers behind | Per-tool timeout and a total tool-time budget per document, 1 MiB console cap, scratch-directory cap (also checked after the tool exits), output-file cap; on Unix the whole process group is killed (so Chrome behind `mmdc` does not linger) |
| Active or unsafe raw HTML (`<script>`, `<a href=javascript:>`, `onclick=`, `<iframe>`, `<object>`, `<svg onload>`, `<form>`...) | HTML is an **allowlist** (see "Raw HTML"): only the listed tags have an effect, attributes are not interpreted beyond the listed ones, scripts/styles/iframes/objects are dropped with their content, and no HTML is ever copied into the document as markup. `<a href>` becomes a link only for `http`/`https`/`mailto`, and not at all with `--untrusted` |
| `---` blocks mid-document silently deleted as "front matter" | Front matter is only recognised at the very start and only if it looks like metadata |

`Config::untrusted()` turns the strict settings on together: native math, diagram
fences left as code (no browser), no local files, smaller limits.

### What is not covered

* **Known content gaps** (each one is reported as a finding, see
  [docs/diagnostics.md](docs/diagnostics.md)). Tables inside footnotes are dropped; a lone
  `\r` inside a code line is removed; `mailto:` links keep their query string
  (`?subject=...`, and in principle `?attach=`, which a few old mail clients honour).
* **`Config::native_math()` is a trusted preset.** It removes the TeX process and nothing else:
  it still reads images from `base_dir`, runs diagram tools and accepts raw HTML links. The
  untrusted policy is `Config::default()` / `Config::untrusted()`.
* **Library checks are not process isolation.** The library bounds work, size, time and files and
  cleans up what it starts, but it cannot cap its own memory, interrupt a callback or a decoder
  that does not return, reap processes after the worker is SIGKILLed, or stop a vulnerability in
  a library it links. Run hostile input in a worker with a memory limit, no network and an
  external supervisor ([docs/embedding.md](docs/embedding.md)).
* **Peak memory is bounded, not small.** The element limit keeps a hostile 2 MiB
  input to a few hundred MB while building; set `Limits::max_elements` lower, and
  run conversions in a memory-limited worker, if that matters for your service.
* **Windows UNC paths.** `//host/share` and `\\host\share` image references are
  refused from the text alone, but this was reasoned from the code and not
  exercised on Windows.

* **Diagram tools are not sandboxed.** There is no seccomp/landlock/chroot around
  them; the protections are the ones listed above (no browser, scrubbed
  environment, caps, kill-on-timeout) plus what the tool itself refuses.
  `mmdc` drives a headless browser with the diagram text and is a much larger
  surface; a custom `--dolphin-command` is whatever you configure. Neither is
  allowed in `untrusted_with_diagrams()`. See "Diagrams from untrusted input"
  for what was verified about merman and what that does and does not promise.
  For stronger isolation run the converter in a container with no network and no
  sensitive files.
* On **Windows** a timed-out tool is killed but its descendants are not (Unix
  uses process groups; a Job Object is not implemented).
* Local-image checks resolve the path and then open it; a hostile process that
  swaps the file between the two steps is outside this threat model.
* Command templates (`--dolphin-command`, ...) and `Config` values are trusted
  configuration, not document content.
* The library default (`Config::default()`) is the untrusted-input policy. The CLI starts
  from `Config::trusted()` because its input is a file you chose; use `--untrusted` otherwise.

Report vulnerabilities as described in [SECURITY.md](SECURITY.md).

## Tests

`cargo test`. CI also runs `cargo fmt --check` and `cargo clippy -- -D warnings`
on Linux, macOS and Windows, an MSRV check (Rust 1.88), and a packaging check.

* Unit tests: LaTeX to OMML (including hostile nesting), namespace injection,
  the security gates, HTML tokenizer, image sizing, command parsing.
* `tests/markdown_coverage.rs`: one case per Markdown construct (above).
* `tests/spec_corpus.rs`: all 652 CommonMark spec examples, default and untrusted
  (valid XML, no script-scheme links, the parser's text all present); `tests/parser_events.rs`:
  every parser tag/event is accounted for (exhaustive matches, so a
  pulldown-cmark upgrade cannot add one silently); `tests/pathological.rs`: deep
  nesting, wide/tall tables, huge lines, delimiter runs, runaway HTML under a
  time budget.
* `tests/highlighting.rs` and the lexer unit tests: every built-in lexer returns valid
  spans on random and hostile text and runs in linear time; themes, custom highlighters
  (including panicking and span-violating ones), round-trip of the code text.
* `tests/code_blocks.rs`: both code layouts give back exactly the source text
  (spaces, tabs, blank lines, markup characters, Unicode), indentation, huge blocks.
* `tests/extensions.rs`: fence renderers, tag styles, document colours, the
  output limit.
* `tests/library_api.rs`: the public API as a downstream crate sees it: page/font
  options, validation, writers, the bounded file reader, concurrent use.
* `tests/security.rs`: every gate in the table, plus seeded random Markdown and
  LaTeX runs (with a watchdog) asserting no panic, no hang, well-formed output.
* `tests/tool_hardening.rs` (Unix): runaway output, disk filling, oversized
  results, descendants of a timed-out, failed or over-budget tool, cancellation, the
  conversion deadline and the scratch root.
* `tests/safe_defaults.rs`, `tests/images_assets.rs`, `tests/diagnostics.rs`,
  `tests/determinism.rs`, `tests/display_math_blocks.rs`: the safe default, host byte assets and
  image budgets, alt text, findings and the strict policy, per-conversion identifiers.
* `tests/office_roundtrip.rs` (Unix, needs LibreOffice, skipped without): every fixture in
  `tests/fixtures/` opens and re-saves in LibreOffice and survives. `tests/mermaid_inprocess.rs`
  (`--features mermaid`): the in-process renderer.
* `tests/tex_real.rs` (Unix): the pipeline and the TeX hardening against a real
  `latex` (skipped when absent; CI installs it and requires it).
* `tests/hardened_diagrams.rs`, `tests/cli_diagrams.rs`: the hardened diagram mode
  (command lock-down, limits, scrubbed environment with a control, CLI switch).
* `tests/merman_hardened.rs`: hostile diagrams against a real `merman-cli`, with
  network and pixel-detector positive controls.
* `tests/merman_real.rs`: the default Mermaid engine against a real `merman-cli`
  (skipped when absent; a dedicated CI job installs and requires it).
* `tests/pipeline.rs`, `tests/pipeline_mock_tools.rs`, `tests/cli.rs`: end-to-end
  conversion, stand-in `mmdc`/`latex`/`dvipng` scripts, and the `md2docx` binary.

Output was also opened and rendered with LibreOffice during development; Word
itself was not available.

## License

MIT. Use it for anything, commercial or not; the only requirement is to keep
the copyright and licence notice (see [LICENSE](LICENSE)) with copies of the
code. Dependencies are all under permissive licences (MIT, Apache-2.0, BSD,
Zlib, Unlicense).
