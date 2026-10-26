# Running md2docx in a container or a worker

`md2docx` bounds the work it does (input, element, image, output and time limits), refuses
dangerous content, and cleans up what it starts. It **cannot** bound its own memory, stop a
kernel-level problem, or isolate itself from the host. That is the container's job. This page is
the recipe.

## What to give the container

| Setting | Why |
| --- | --- |
| A non-root user | Nothing in the converter needs privileges. |
| `--read-only` root filesystem plus a small writable tmpfs for `--scratch-dir` | Output goes to standard output or a mounted directory; scratch space is only used when external renderers run. |
| `--network none` | The converter never uses the network; diagram tools and TeX must not either. |
| `--memory`, `--pids-limit`, `--cpus` | The converter cannot cap its own memory. A document that is too big for the worker is killed (status 137): treat that as "reject, too big". Pair it with `--max-elements`. |
| `--init` | Reaps any process an external renderer leaves behind. |
| `--cap-drop ALL`, `--security-opt no-new-privileges` | Defence in depth. |
| An outer timeout (`timeout 150 …`) | `--deadline` is cooperative; a hard kill is yours. Keep it a little longer than `--deadline`. |

## A hostile-input worker, no mounts

```sh
docker run --rm -i --init --network none --read-only \
  --tmpfs /scratch:rw,size=64m --memory 512m --pids-limit 64 --cpus 1 \
  --cap-drop ALL --security-opt no-new-privileges \
  -e MD2DOCX_UNTRUSTED=1 -e MD2DOCX_STRICT=1 -e MD2DOCX_FORMAT=json \
  -e MD2DOCX_DEADLINE=30 -e MD2DOCX_MAX_INPUT_BYTES=1M -e MD2DOCX_SCRATCH_DIR=/scratch \
  -e SOURCE_DATE_EPOCH=0 \
  silvercity-oss/md2docx:local - < upload.md > result.docx 2> report.json
echo "status $?"
```

* The document goes in on standard input and out on standard output; nothing is mounted.
* `report.json` is one line of JSON (see [cli.md](cli.md)). Branch on the status: `0` deliver,
  `4` and `5` tell the author (their document), `6`/`3`/`1` page an operator, `7` retry or give
  up, `137` too big.
* `MD2DOCX_UNTRUSTED=1` means no TeX, no diagram process, no local files, tight limits. Limits can
  be lowered by the environment but not raised past that preset.
* Add `--diagrams` for Mermaid; in an image built with the `mermaid` feature (below) the renderer
  runs inside the process, so the container still starts nothing.

## The image

The repository's `Dockerfile` builds a static-ish image with the in-process Mermaid renderer and
nothing else:

```sh
docker build -t silvercity-oss/md2docx:local .
```

It is a multi-stage build (`rust` → `gcr.io/distroless/cc`), runs as the unprivileged `nonroot`
user, and has `md2docx` as its entry point. Rust 1.95 or later is needed for the `mermaid` feature
(merman's own requirement).

## Your own documents (trusted input)

```sh
docker run --rm --network none --user "$(id -u)" -v "$PWD:/work" -w /work \
  silvercity-oss/md2docx:local docs/*.md -o build/
```

The default image has no TeX or `merman-cli`/`mmdc`, so `--math-mode image` and the external
Mermaid engines are not available there; native equations and `--mermaid-engine inprocess` are.
For image-mode math, build an image that adds `texlive-latex-base` and `dvipng`.

## CI

```yaml
- run: md2docx --check --strict --format json docs/*.md 2>docs-report.jsonl
```

`--check` writes nothing, so it also works on a read-only checkout. Status `5` is "a document
loses something in Word"; the JSON lists each finding with line and column, ready for annotations.

## In-process embedding instead

If your service is written in Rust, call the library directly; see [embedding.md](embedding.md).
The same advice applies: run it in a worker with a memory limit and a hard timeout, because the
library cannot cap its own memory.
