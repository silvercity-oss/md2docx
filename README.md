# silvercity-oss

A pure-Rust library (plus the `md2docx` CLI) that converts Markdown into
high-fidelity Word `.docx` files, including **LaTeX math** and **Mermaid /
Dolphin diagrams** that `docx-rs` cannot express natively.

```rust
use silvercity_oss::{markdown_to_docx, Config};

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
md2docx notes.md                                  # -> notes.docx, native equations
md2docx notes.md -o out.docx --math-mode image    # PNG formulas via latex + dvipng
md2docx notes.md --dolphin-command 'dolphin -i {input} -o {output}'
md2docx notes.md --mermaid-command 'mmdc -i {input} -o {output} -s 2 -p puppeteer.json'
```

| Flag | Meaning |
| --- | --- |
| `<input>` | Markdown file (relative image paths resolve against its directory) |
| `-o, --output` | Output path (default: `<input>.docx`; refuses to overwrite the input) |
| `--math-mode native\|image` | Default **`native`** (editable OMML, no subprocess). `image` needs a TeX install |
| `--dolphin-command`, `--mermaid-command` | Command templates, split with shell quoting rules; must contain `{input}` and `{output}` (`{dpi}` optional) |
| `--latex`, `--dvipng` | Executables for `image` math (e.g. `xelatex`) |
| `--timeout <seconds>` | Kill a renderer that runs longer (default 60) |

Note the CLI defaults to `native` math while the library's `Config::default()`
uses `ImageFallback`; use `Config::native_math()` for the safe mode. On Windows,
npm-installed tools are `.cmd` shims, so pass `mmdc.cmd` explicitly.

## Security

> **Security Advisory Notice: Image Mode runs a local TeX engine on document content.**
>
> `MathMode::ImageFallback` (CLI: `--math-mode image`) invokes `latex` on a
> document generated from the formulas in the Markdown. The crate passes
> `-no-shell-escape`, which blocks `\write18` command execution, but that flag
> does **not** make TeX safe to feed untrusted text: active TeX macros such as
> `\input{...}`, `\include`, or `\openin`/`\read` can read files on the host
> (for example `\input{/etc/passwd}`), and their contents are typeset into the
> rendered formula image, which then ends up in the generated `.docx`. Mermaid
> rendering (`mmdc`) likewise drives a headless browser with the diagram text.
>
> **Native OMML Mode must be mandated whenever the Markdown comes from an
> untrusted source** (user uploads, web forms, third-party feeds, multi-tenant
> servers). It converts LaTeX to OMML in-process, never spawns `latex`, and
> cannot read host files. With `md2docx` it is already the default; in library
> code use `Config::native_math()`. If diagrams must also be handled for
> untrusted input, run `mmdc`/`dolphin` inside a sandbox (container, no
> network, no access to sensitive paths) and keep `tool_timeout` low.

Other hardening in the crate: external tools run with no shell (arguments are
passed as an argv list, never interpolated into a command line), are killed on
timeout, and all scratch files are removed when a conversion ends; image
dimensions are bounds-checked (header size limit, checked arithmetic) so a
hostile PNG header cannot cause a panic or overflow; percent-encoded image paths
are decoded with `percent-encoding` and invalid UTF-8 is replaced rather than
sliced. Local image references in Markdown (`![x](/etc/shadow.png)`) are read
from disk and embedded if they decode as images, so treat that as file access
too when converting untrusted Markdown.

## Tests

`cargo test` (CI also runs `cargo fmt --check` and `cargo clippy -- -D warnings`
on Linux, macOS and Windows).

* `src/**` unit tests: LaTeX to OMML, namespace injection, image sizing, path
  decoding, command parsing.
* `tests/pipeline.rs`: tool-free end-to-end tests (native OMML, Markdown
  structure, errors, local images). Run on every OS.
* `tests/pipeline_mock_tools.rs` (Unix only): installs tiny stand-in
  `mmdc`/`latex`/`dvipng` shell scripts and verifies what was passed to them,
  that the PNGs end up in the package, error variants, timeouts, caching, and that
  temp files are gone afterwards.
* `tests/cli.rs`: runs the real `md2docx` binary (flags, exit codes, messages).

The produced documents were also opened and rendered with LibreOffice during
development; Word itself was not available.
