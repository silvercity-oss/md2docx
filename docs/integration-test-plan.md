# Integration test plan

What is verified by this repository's own tests, and what an application embedding the converter
should verify in its own environment. Each item is marked **Automated** (runs in `cargo test` or CI
here) or **Proposed** (to be run by the embedder; not executed by this repository).

## 1. Fixtures

`tests/fixtures/*.md` are the sources; `cargo run --example fixtures -- <dir>` writes them as
`.docx` (images supplied through an `AssetMap`, the diagram drawn, or rendered by merman with
`--features mermaid`).

| File | Exercises |
| --- | --- |
| `01-structure` | Front matter, headings, emphasis, links, nested/ordered/bullet/task lists, block quote, table with alignments, code block, rule |
| `02-math` | Inline and display OMML equations, a display equation inside a numbered list item |
| `03-images` | Host-supplied images, alt text with hostile characters, a title, the same asset used twice |
| `04-footnotes` | Native footnotes with inline formatting |
| `05-diagram` | A diagram fence with an author-written `alt=` |
| `06-lossy` | Remote image, relative link, missing anchor, unknown LaTeX command, raw `<script>`, unreferenced footnote: every one is a finding (strict mode must reject this file) |
| `07-contents` | Static contents, numbered headings, content-sized columns, autolinks, a footnote with a link |

## 2. Automated checks in this repository

| Concern | Where | What it proves |
| --- | --- | --- |
| Safe default | `tests/safe_defaults.rs` | `Config::default()` reads no file, runs no tool, keeps diagrams as code, uses native math; `native_math()` is not the untrusted preset |
| Host byte assets | `tests/images_assets.rs` | Exact bytes supplied by key with file access off; keys never opened as paths; one resolver call per key; failures and panics are errors |
| Image limits | `tests/images_assets.rs`, `src/images.rs` | Source size, distinct-image count, per-image pixels/dimension, aggregate decode pixels (different encodings of the same pixels each pay; exact repeats are free), aggregate normalised bytes; the example "strict profile" (1 MiB source, 50 images, 4 M px, 16 M decoded, 8 MiB normalised, 16 MiB output) at its boundaries |
| Normalisation | `tests/images_assets.rs` | PNG, JPEG, GIF are decoded and re-embedded as clean PNG; appended payloads do not survive |
| Accessible images | `tests/images_assets.rs`, `tests/pipeline_mock_tools.rs`, `tests/office_roundtrip.rs` | `descr`/`title` on `wp:docPr` (escaped), unique drawing ids, diagram/formula descriptions, `alt=` override |
| Diagnostics | `tests/diagnostics.rs`, `src/diagnostics.rs` | Every construct in `docs/diagnostics.md` yields its code, severity and exact original-source span (through BOM, CRLF, front matter, rewritten fences and formulas); strict mode rejects; info never rejects; `analyze` agrees with `convert`; the reference lists every code |
| Process cleanup | `tests/tool_hardening.rs` | Timeout, failure, success, console flood, over-budget workspace, **fast exit + leftover helper + over-budget workspace**, console limit after exit, and all of them combined: the helper process is dead and the scratch directory is gone (Unix) |
| Cancellation and deadline | `tests/tool_hardening.rs`, `tests/mermaid_inprocess.rs`, `src/control.rs` | A running tool and its helpers are killed on cancel and on deadline; pre-cancelled token does no work; an in-process render is interrupted |
| Scratch root | `tests/tool_hardening.rs`, `tests/cli.rs`, `tests/mermaid_inprocess.rs` | Tool cwd, input and output are under the host root; removed after success and after errors; missing root is an error; renderer bytes never touch it; **a force-killed CLI leaves exactly one job directory and `sweep_scratch` recovers it** |
| In-process Mermaid | `tests/mermaid_inprocess.rs` (`--features mermaid`) | No process is spawned (the configured command does not exist); output scaled to the image limits; merman budget → `LimitExceeded` that `KeepAsCode` does not hide; parse errors follow the failure policy |
| Office round trip | `tests/office_roundtrip.rs` | LibreOffice opens each fixture, saves it as `.docx`, and the saved file reopens; text, tables, equations, footnotes, pictures and alt text survive (skipped without `soffice`; `REQUIRE_SOFFICE=1` makes absence a failure) |
| Word-strict schema check | `tools/validate-output.sh`, `tools/ooxml-validate` (CI job `ooxml`) | The fixtures and this repository's Markdown, converted with a spread of options, validate against the Office Open XML schemas with Microsoft's Open XML SDK; Word is stricter than LibreOffice about element order and unknown attributes |
| Fuzzing | `fuzz/` (CI job `fuzz`) | `cargo fuzz` targets for the whole pipeline, the LaTeX → OMML converter and raw HTML/tables/footnotes; thirty seconds per target on every push, ten minutes weekly |
| Windows process containment | `tests/tool_hardening_windows.rs` | A timed-out tool's helper stops running (Job Object) |
| EXIF orientation | `src/images.rs` | A JPEG that says "rotate a quarter turn" is stored with its sides swapped |
| Determinism | `tests/determinism.rs` | Identifiers do not depend on earlier or concurrent conversions |
| Display equations | `tests/display_math_blocks.rs` | A block equation keeps its list marker, quote indent and task checkbox |
| Hostile input | `tests/security.rs`, `tests/pathological.rs`, `tests/spec_corpus.rs`, `tests/audit_fixes.rs` | Limits, XML integrity, path containment, link filtering, every CommonMark example, pathological nesting |

CI runs these on Linux, macOS and Windows (process-based tests are Unix-only), on the MSRV (1.88),
and the `mermaid` feature on Rust 1.95.0 and stable.

## 3. Proposed validation for an embedding application

Not run by this repository. Run them against your real worker and storage.

1. **Worker downloads.** Convert through the real request path with a content store that returns
   exact bytes: confirm that the delivered file's `word/media/*` are byte-identical to what your
   store normalised, that no path or URL from the document was ever opened (log resolver calls;
   assert a file-system and network sandbox), and that cached or no-store response headers are
   preserved by your layer.
2. **Resource limits.** Run the worker under your real limits (address space, CPU, wall clock,
   descriptors, process count). Feed the fixtures plus: a 10 000-row table, 60 000 paragraphs, a PNG
   whose header claims 40 000 x 40 000, 200 distinct 4-megapixel images, and a diagram at merman's
   limit. Expect `CrateError::LimitExceeded` (or your supervisor's kill), never a hang; record peak
   RSS and compare with your budget.
3. **Cancellation.** Cancel at start, during an image decode, during a diagram render and during a
   packaging step; measure time to return. Separately SIGKILL the worker mid-conversion and confirm
   your reaper removes the job directory (or `sweep_scratch` does) and any child process.
4. **Cleanup.** After 1,000 mixed successes and failures, the scratch root must be empty and no
   `md2docx-*` entry may remain.
5. **Accessibility.** Open each fixture in Word and run **Review → Check Accessibility**: pictures must
   show alt text, none should be flagged "missing alt text" unless the Markdown had none. Read
   `03-images` and `05-diagram` with a screen reader (Narrator/NVDA).
6. **Word and LibreOffice open–edit–save–reopen.** For each fixture: open in Word (and again in
   LibreOffice), type into a paragraph, add a row to the table, edit an equation, change a picture's
   alt text, delete a footnote, save as `.docx`, close, reopen. Expect no repair prompt, equations
   still editable, list numbering continuous, footnotes renumbered. LibreOffice's save-and-reopen
   without edits is automated in section 2; **Microsoft Word was not available in the development
   environment, so every Word step is unexecuted.**
7. **Preview consistency.** For representative accepted documents, compare your own semantic
   preview's counts with `analyze(...).stats` and your validation messages with the diagnostics;
   a difference means your preview and the worker disagree about what the document contains.
8. **Strict mode in production.** Run your corpus of real documents with `LossPolicy::Reject` in
   shadow mode (log, do not block) and review the code histogram before enforcing it.
