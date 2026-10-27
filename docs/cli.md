# The `md2docx-cli` command line

This page is the specification of the command-line interface. **The exit status, the standard
streams and the JSON report are interfaces you may script against**; they are tested by number
(`tests/cli_contract.rs`) and will not change meaning within a major version. Flags may be added.

```text
md2docx-cli [OPTIONS] <INPUT>...        # INPUT is a file, or `-` for standard input
```

## Exit status

The status says **who has to act**, so a wrapper can branch with one `case` on `$?`.

| Status | Name | Meaning | Who acts / retry |
| ---: | --- | --- | --- |
| `0` | ok | The document was converted (with `--check`: found convertible). Warnings may have been printed. | |
| `1` | failure | Anything not listed below, including a bug in `md2docx-cli` and a cancelled conversion. | Operator; report it if it persists. |
| `2` | usage | The command line or its configuration is wrong: unknown flag, conflicting options, a limit that `--untrusted` forbids raising, an impossible page size, an unreadable `--reference-doc`, `--deadline 0`, two inputs that would write the same file, writing a `.docx` to a terminal, overwriting the input. Nothing was converted. | The person who wrote the command. Do not retry. |
| `3` | i/o | A file could not be read or written: the input, the output, the output directory, a standard stream. The document itself was not judged. | Operator. A retry can succeed. |
| `4` | rejected | The **document** is at fault: malformed (invalid math, not UTF-8), over a limit (`--max-*`, the preset's), or forbidden by the security policy (a path outside `--base-dir`). | Whoever wrote the Markdown. The same input gives the same answer: do not retry unchanged. |
| `5` | lossy | `--strict`: the document is valid but could not be converted without loss, so **nothing was written**. The findings were reported. | Whoever wrote the Markdown, or drop `--strict`. |
| `6` | tool | An external program is missing or failed: `latex`/`dvipng` (image math), `mmdc`/`merman-cli`, a custom renderer, or a per-tool `--timeout`. | Operator (install it, fix the environment). If the renderer *rejected a diagram*, `--keep-failed-diagrams` keeps it as code. |
| `7` | timeout | `--deadline` passed before the conversion finished. | A retry with more time, or on a quieter machine, may succeed. |

Unix statuses above 128 are the shell's report of a signal (137 = killed, 134 = aborted) and are
not produced by `md2docx-cli` itself. A process that runs out of memory is killed by the system, not
reported: give the worker a memory limit and treat 137 as "too big for this worker".

* With several inputs every input is tried; the status is that of the **first one that failed**
  (use `--format json` for the per-file result).
* Statuses `8` and above are reserved. Treat an unknown non-zero status as a failure (`1`).
* `--check` uses the same statuses; it simply writes nothing.
* The fine-grained reason is in the JSON report's `error.kind`, which is more detailed than the
  status (`invalid_markdown`, `limit_exceeded` and `unsafe_content` all exit `4`).

## Standard streams

| Stream | Content |
| --- | --- |
| standard output | Only the `.docx` bytes, and only when the output is `-`. (Also `--help`, `--version`, `--completions`, `--man`.) |
| standard error | Everything else: warnings, the `wrote …` line, errors, or with `--format json` the JSON report. |

* `md2docx-cli doc.md` writes `doc.docx` next to the input and prints `wrote doc.docx`.
* `md2docx-cli - < doc.md > out.docx` and `-o -` write the document to standard output. If standard
  output is a terminal, `md2docx-cli` refuses (status 2) instead of printing a ZIP archive at you.
* `md2docx-cli - -o out.docx` reads standard input and writes a file.
* Input from standard input is read up to `--max-input-bytes` and must be UTF-8. Relative images
  resolve against `--base-dir`, or the current directory.
* The output file is written to a temporary file and renamed, so a failure never leaves a
  half-written `.docx`; a symbolic link at the output path is replaced, not followed.

### Text format

```text
notes.md:12:5: warning[link.dropped]: the link target `a/b.md` is not an allowed destination; only its text was kept
wrote notes.docx
```

`file:line:column: severity[code]: message`, the shape editors and CI annotators understand.
`--quiet` removes warnings and the `wrote` line; errors always print. Codes are listed in
[diagnostics.md](diagnostics.md).

## JSON report (`--format json`)

With `--format json` standard error carries **exactly one line of JSON per input** and nothing
else (usage errors that clap reports before it knows the format are the one exception: they are
text, status 2). `--quiet` has no effect on it.

```json
{"schema":1,"input":"notes.md","output":"notes.docx","origin":"input","status":"ok","exit_code":0,"error":null,
 "diagnostics":[{"code":"link.dropped","severity":"warning","message":"…",
                 "span":{"start":10,"end":22,"line":2,"column":5,"end_line":2,"end_column":17}}],
 "stats":{"headings":1,"tables":0,"pictures":0,"equations":0,"footnotes":0,"hyperlinks":2,"code_blocks":0}}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `schema` | number | `1`. Changes only for a breaking change of this format. Fields may be **added** within a schema version; ignore ones you do not know. |
| `input` | string or null | The input path; `"-"` for standard input; `null` when the failure came before any input was read (a usage error). |
| `output` | string or null | The output path; `"-"` for standard output; `null` for `--check` and for failures. |
| `origin` | `"input"` \| `"link"` | `"link"` for a Markdown file converted because a document linked to it (`--markdown-links follow`). |
| `status` | `"ok"` \| `"error"` | |
| `exit_code` | number | The status this input contributes (see above). |
| `error` | null or object | `{"kind": …, "message": …}` on failure. `message` is for people. |
| `diagnostics` | array | Findings: `code` (stable, see diagnostics.md), `severity` (`"warning"` \| `"info"`), `message` (for people), `span` (null, or byte offsets and 1-based line/column into the **original** Markdown; `end` exclusive). On a `--strict` refusal these are the findings behind it. |
| `stats` | object or null | Counts of what the document contains; `null` on failure. |

`error.kind` values: `usage`, `invalid_config`, `io`, `invalid_markdown`, `limit_exceeded`,
`unsafe_content`, `lossy_content`, `tool_missing`, `tool_failed`, `timed_out`, `cancelled`,
`internal`. New kinds may appear; branch on `exit_code` first.

## Validating without producing a file

`md2docx-cli --check doc.md` runs the whole pipeline (same limits, same rendering, same findings) and
writes nothing: use it as a CI lint step. By itself, findings are reported but the status is `0`;
add `--strict` to make any warning status `5`. Because no file is produced, the output-size
limit is not exercised.

```sh
md2docx-cli --check --strict --format json docs/*.md 2>report.jsonl || echo "docs need fixing"
```

## Several inputs

```sh
md2docx-cli a.md b.md c.md                 # a.docx, b.docx, c.docx next to the inputs
md2docx-cli docs/*.md -o build/            # build/<name>.docx; the directory must exist
```

Two inputs that would write the same output file are refused before anything runs (status 2). With `--markdown-links follow` the list grows as links are followed; see [links.md](links.md).
`-` cannot be combined with other inputs, and `-o -` takes a single input.

## Configuration

Flags are the interface. For containers, the operational ones can also come from the environment;
**a flag on the command line wins**. Settings that *loosen* the security policy (`--math-mode`,
`--image-access`, `--image-encoding`, `--mermaid-*`, `--dolphin-command`, `--latex`, `--dvipng`, `--reference-doc`) have no environment
variable on purpose, so the ambient environment cannot quietly widen what a document may do.

| Environment variable | Flag |
| --- | --- |
| `MD2DOCX_UNTRUSTED` | `--untrusted` |
| `MD2DOCX_STRICT` | `--strict` |
| `MD2DOCX_FORMAT` | `--format` |
| `MD2DOCX_QUIET` | `--quiet` |
| `MD2DOCX_DEADLINE` | `--deadline` |
| `MD2DOCX_TIMEOUT` | `--timeout` |
| `MD2DOCX_BASE_DIR` | `--base-dir` |
| `MD2DOCX_SCRATCH_DIR` | `--scratch-dir` |
| `MD2DOCX_MAX_INPUT_BYTES`, `MD2DOCX_MAX_OUTPUT_BYTES`, `MD2DOCX_MAX_UNCOMPRESSED_BYTES`, `MD2DOCX_MAX_ELEMENTS`, `MD2DOCX_MAX_IMAGE_BYTES`, `MD2DOCX_MAX_IMAGES` | `--max-…` |
| `SOURCE_DATE_EPOCH` | `--source-date-epoch` |

Booleans accept `1`/`true`/`yes`/`on` and `0`/`false`/`no`/`off`. Sizes take a binary suffix
(`2M` = 2 MiB, `64K`, `1G`).

### Limits and `--untrusted`

`--untrusted` selects the hostile-input preset. The limit flags may be combined with it, but only
to **lower** a limit: `--untrusted --max-input-bytes 1M` is fine, `--untrusted --max-input-bytes
1G` is a usage error (status 2). The same holds for `--deadline` and for `--link-schemes`, which
may only be narrowed. Without `--untrusted` any value is accepted.

| Flag | Default | Under `--untrusted` |
| --- | --- | --- |
| `--max-input-bytes` | 10 MiB | 2 MiB |
| `--max-output-bytes` | 1 GiB | 50 MiB |
| `--max-uncompressed-bytes` | 1 GiB | 64 MiB |
| `--max-elements` | 1 000 000 | 60 000 |
| `--deadline` | none | 120 s |

### Reproducible output

`SOURCE_DATE_EPOCH=… md2docx-cli doc.md` (or `--source-date-epoch`) fixes the created and modified
times; with it, the same input and the same version give the same bytes. Without it, the time of
the conversion is recorded.

## Flag reference

`md2docx-cli --help` is authoritative; `md2docx-cli --man` prints a manual page and
`md2docx-cli --completions bash|zsh|fish|powershell|elvish` a completion script.

| Group | Flags |
| --- | --- |
| Input / output | `INPUT…`, `-o/--output`, `--check`, `--format text\|json`, `-q/--quiet`, `--strict` |
| Policy | `--untrusted`, `--diagrams`, `--image-access`, `--image-encoding`, `--base-dir`, `--link-schemes`, `--no-html-links`, `--math-mode`, `--keep-failed-diagrams` |
| Renderers | `--mermaid-engine merman\|mmdc\|inprocess`, `--mermaid-command`, `--dolphin-command`, `--latex`, `--dvipng`, `--timeout`, `--scratch-dir` |
| Limits | `--deadline`, `--max-input-bytes`, `--max-output-bytes`, `--max-uncompressed-bytes`, `--max-elements`, `--max-image-bytes`, `--max-images` |
| Document | `--title`, `--author`, `--page-size`, `--landscape`, `--margin`, `--body-font`, `--body-size`, `--code-font`, `--page-numbers`, `--reference-doc`, `--number-headings`, `--toc`, `--code-blocks`, `--highlight`, `--compression-level`, `--source-date-epoch` |
| Markdown extras | `--autolink`, `--sup-sub` |
| Links to files | `--allow-links none\|local\|all`, `--link-root`, `--link-style rebased\|absolute`, `--link-base`, `--link-tree-base`, `--link-map`, `--no-check-link-anchors`, `--markdown-links keep\|docx\|follow`, `--link-layout mirror\|flat`, `--copy-linked`, `--max-linked-files`, `--max-copy-bytes` (see [links.md](links.md)) |

`--mermaid-engine inprocess` exists only in builds with the `mermaid` feature
(`cargo install md2docx --features mermaid`; `md2docx-cli --version` says whether it is compiled
in). There `--untrusted --diagrams` also renders in-process: no child process at all.
