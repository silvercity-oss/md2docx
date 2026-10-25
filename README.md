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

| Input | Result in Word |
| --- | --- |
| Headings (ATX, setext, `{#id .class}` attributes) | Heading 1-6 styles; attributes dropped |
| Emphasis, strong, strikethrough, inline code, hard/soft breaks, escapes, entities | Character formatting |
| Links (inline, reference, autolink `<url>`, `<email>`) | Hyperlinks, **only for `http`, `https`, `mailto`** (see Security); other targets keep their text |
| Block quotes (nested), GitHub alerts `> [!NOTE]` | Indented with a bar; alerts get a bold label line |
| Bullet / ordered / nested / task lists, start numbers | Native Word numbering (each ordered list restarts); `☐`/`☑` for tasks |
| Tables (alignment, inline formatting) | Word tables |
| Fenced, tilde and indented code | Monospace shaded lines |
| Thematic breaks | A rule |
| Footnotes `[^1]` | Native Word footnotes (unknown labels stay literal) |
| Definition lists | Bold term, indented definitions |
| YAML / TOML front matter | Dropped, but only at the very start of the file and only if it looks like metadata. `---` blocks elsewhere are normal content |
| Raw HTML | `<b> <i> <u> <s> <sup> <sub> <code> <kbd> <br> <hr> <h1-6> <li> <img>` and block elements are rendered; scripts, styles, comments, iframes and unknown tags contribute only their text |
| Images | Local PNG/JPEG/GIF scaled to the text width, subject to the image policy; SVG, WebP and corrupt images, and all remote or `data:` images become `[alt text]` (nothing is fetched) |
| `$..$` / `$$..$$` math, `mermaid` / `dolphin` fences | See above |

Not supported: HTML `<a href>` links (their text is kept), bare-URL autolinking, wiki links,
super/subscript via `^x^`/`~x~` (use `<sup>`/`<sub>`), `<table>` layout (cell text
is kept), math inside footnotes (shown as source text).

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
| `--untrusted` | **Use this for input you do not control.** Native math, diagram fences stay code, no local images, tighter limits; starts no subprocess. Cannot be combined with switches that would loosen it |
| `--image-access none\|base-dir\|any` | Default `base-dir`: only images inside the input file's directory |
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
> cannot read host files. In library code use `Config::untrusted()` (or at least
> `Config::native_math()`); with the CLI use `md2docx --untrusted`.
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
| Huge input, many blocks, footnote amplification | `Limits`: input size, number of diagrams/formulas, formula and diagram size, image and workspace bytes, footnote expansion |
| A renderer that hangs, floods output, fills the disk or leaves helpers behind | Timeout, 1 MiB console cap, scratch-directory cap, output-file cap; on Unix the whole process group is killed (so Chrome behind `mmdc` does not linger) |
| Script/style/iframe content in raw HTML | Dropped, including when split across inline events |
| `---` blocks mid-document silently deleted as "front matter" | Front matter is only recognised at the very start and only if it looks like metadata |

`Config::untrusted()` turns the strict settings on together: native math, diagram
fences left as code (no browser), no local files, smaller limits.

### What is not covered

* **Diagram tools are not sandboxed.** `mmdc` drives a headless browser with the
  diagram text, and a custom `--dolphin-command` is whatever you configure. If
  untrusted users may trigger diagram rendering, run the converter itself inside
  a container with no network and no sensitive files, or leave diagrams disabled
  (`--untrusted`).
* On **Windows** a timed-out tool is killed but its descendants are not (Unix
  uses process groups; a Job Object is not implemented).
* Local-image checks resolve the path and then open it; a hostile process that
  swaps the file between the two steps is outside this threat model.
* Command templates (`--dolphin-command`, ...) and `Config` values are trusted
  configuration, not document content.
* The library default (`Config::default()`) keeps the spec'd image mode for
  math; the CLI defaults to native.

Report vulnerabilities as described in [SECURITY.md](SECURITY.md).

## Tests

`cargo test`. CI also runs `cargo fmt --check` and `cargo clippy -- -D warnings`
on Linux, macOS and Windows, an MSRV check (Rust 1.88), and a packaging check.

* Unit tests: LaTeX to OMML (including hostile nesting), namespace injection,
  the security gates, HTML tokenizer, image sizing, command parsing.
* `tests/markdown_coverage.rs`: one case per Markdown construct (above).
* `tests/security.rs`: every gate in the table, plus seeded random Markdown and
  LaTeX runs (with a watchdog) asserting no panic, no hang, well-formed output.
* `tests/tool_hardening.rs` (Unix): runaway output, disk filling, oversized
  results, and descendants of a timed-out tool.
* `tests/tex_real.rs` (Unix): the pipeline and the TeX hardening against a real
  `latex` (skipped when absent; CI installs it and requires it).
* `tests/pipeline.rs`, `tests/pipeline_mock_tools.rs`, `tests/cli.rs`: end-to-end
  conversion, stand-in `mmdc`/`latex`/`dvipng` scripts, and the `md2docx` binary.

Output was also opened and rendered with LibreOffice during development; Word
itself was not available.

## License

MIT. Use it for anything, commercial or not; the only requirement is to keep
the copyright and licence notice (see [LICENSE](LICENSE)) with copies of the
code. Dependencies are all under permissive licences (MIT, Apache-2.0, BSD,
Zlib, Unlicense).
