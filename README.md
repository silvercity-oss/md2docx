# ReqMech Companion

A pure-Rust library (plus a small `reqmech` CLI) that converts Markdown into
high-fidelity Word `.docx` files, including **LaTeX math** and **Mermaid /
Dolphin diagrams** that `docx-rs` cannot express natively.

```rust
use reqmech_companion::{markdown_to_docx, Config};

let docx: Vec<u8> = markdown_to_docx("# Hi\n\n$$E = mc^2$$", &Config::native_math())?;
std::fs::write("out.docx", docx)?;
```

`markdown_file_to_docx(&Path, &Config)` does the same for a file and resolves
relative image paths against its directory. Every failure is a
`CrateError` (`MissingExternalCLI`, `InvalidMarkdown`, `DocxGenerationError`,
`ExternalToolFailed`, `Io`); the crate does not panic on bad input.

## Pipeline

```
Markdown ──► 1. preprocess ──► 2. pulldown-cmark events ──► 3. docx-rs writer ──► .docx bytes
             (render, rewrite)                              (+ OMML injection)
```

| Module | Role |
| --- | --- |
| `preprocess.rs` | Finds `mermaid`/`dolphin` fences and `$`/`$$` math with `pulldown-cmark` (`ENABLE_MATH`, so code spans, escapes and nested quotes/lists are handled by the real parser), renders them, and rewrites each into `![alt](dest)`. |
| `renderer.rs` | Runs external CLIs (`std::process::Command`, timeout, stderr capture), names assets by SHA-256, owns the RAII `Workspace` (`tempfile::TempDir`). |
| `omml.rs` | Dependency-free LaTeX → OMML converter. |
| `parser.rs` | The event loop mapping CommonMark events onto `docx-rs`, and the OMML injection step. |

Everything rendered lives in one `TempDir` that is deleted when the conversion
returns, on success and on every error path.

## Diagrams

`mermaid` fences are rendered with `mmdc` (`-s 3 -b transparent`), `dolphin`
fences with the configured Dolphin command. Both are `CommandSpec`s: a program
plus arguments with `{input}`, `{output}` and `{dpi}` placeholders.
There is no standard Dolphin CLI; the default `dolphin -i {input} -o {output}`
is a guess, so set `config.graphs.dolphin` for your tool. Identical diagrams
are rendered once. On a headless/root host Mermaid usually needs a Puppeteer
config (`-p puppeteer.json` with `--no-sandbox`); add it to `args`.

## Math modes (`config.math.mode`)

* **`MathMode::ImageFallback`** (default): each formula becomes a transparent
  PNG. Default engine is `latex` + `dvipng -T tight -bg Transparent`
  (TeX Live); `MathImageEngine::Command(..)` plugs in anything else (a
  MathJax/headless-browser node script, …) that reads LaTeX from `{input}` and
  writes a PNG to `{output}`; `{display}` is `1`/`0`.
* **`MathMode::NativeOmml`**: no external tools. `omml.rs` converts LaTeX to
  OMML, and `parser.rs` swaps placeholder runs in the rendered
  `document.xml` for `<m:oMath>` / `<m:oMathPara>` before zipping (docx-rs
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

Headings, paragraphs, emphasis/strong/strikethrough, inline code, links
(external hyperlinks), block quotes, nested bullet/ordered lists (ordered lists
restart correctly), task lists, tables with alignment, fenced code, rules,
local images (PNG/JPEG/GIF, scaled to the text width), YAML front matter
(skipped). Remote images become `[alt]` text (no network access); raw HTML is
dropped except `<br>`; a missing local image is an `Io` error.

## CLI

```
reqmech notes.md -o notes.docx --math omml
reqmech notes.md --mmdc /opt/bin/mmdc --latex latex --dvipng dvipng
```

## Tests

`cargo test`. The integration tests (`tests/pipeline.rs`) install tiny stand-in
`mmdc`/`latex`/`dvipng` shell scripts, so they run without those tools and
verify what was passed to them, that the PNGs end up in the package, that the
OMML ends up in `document.xml` (well-formed), the error variants, timeouts, and
that temp files are gone afterwards. The produced documents were also opened
and rendered with LibreOffice during development; Word itself was not available.

## Security note

Image mode feeds Markdown content to `latex`/`mmdc`. `latex` runs with
`-no-shell-escape`, but TeX can still `\input` local files, so don't render
untrusted input with the image engine on a host with sensitive files; native
OMML mode runs no subprocess.
