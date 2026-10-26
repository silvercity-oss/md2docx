# Embedding silvercity-oss in a service

This guide is for programs that convert Markdown written by *other people* (uploads, form fields, a
content store) and want to deliver a `.docx`. It says what to configure, what the library enforces
itself, and what only the process around it can enforce. A runnable version of the recipe is
`examples/embed.rs`.

## Who controls what

| Controlled by | What |
| --- | --- |
| **The document** (untrusted) | The Markdown text, including image keys, link targets, diagram and formula source, fence info strings (`alt=`, `title=`) |
| **The embedding program** (trusted) | The whole `Config`; every `ImageResolver`/`AssetMap`; every `FenceRenderer`; highlighters; custom HTML tag styles; command templates; the scratch directory; the cancel token; limits |

A document can never choose a renderer, a path, a command, a limit or a scratch location. Everything
in the second row is written by you, so a bug in it is a bug in your program, not an attack
surface of the converter. In particular, a `FenceRenderer` or `ImageResolver` receives untrusted
text (the diagram source, the image key): look keys up, never open them as paths.

## The recipe

```rust
use std::time::Duration;
use silvercity_oss::{convert, AssetMap, CancelToken, Config, CrateError, LossPolicy};

fn config(job_dir: &std::path::Path, images: AssetMap, cancel: CancelToken) -> Config {
    let mut cfg = Config::default()          // 1. the untrusted-input policy
        .with_image_resolver(images)         // 2. exact bytes the host accepted
        .with_loss_policy(LossPolicy::Reject)// 3. never deliver a degraded document
        .with_scratch_dir(job_dir)           // 4. temp files only under the host's job directory
        .with_cancel_token(cancel)           // 5. the host can stop it
        .with_deadline(Duration::from_secs(30));
    cfg.limits.max_image_bytes = 1024 * 1024;       // 6. your own ceilings (see below)
    cfg.limits.max_output_bytes = 16 * 1024 * 1024;
    cfg
}
```

* `Config::default()` is `Config::untrusted()`: native (OMML) math, **no** TeX process, **no**
  diagram process, **no** local files, raw-HTML links dropped, tight limits, no subprocess ever.
  `Config::native_math()` is *not* that: it is `Config::trusted()` with native equations, and still
  reads files and runs diagram tools. Start from `default()` and add only what you need.
* TeX, external renderers and local-file access exist only behind explicit trusted configuration
  (`Config::trusted()`, `MathMode::ImageFallback`, `GraphOptions::enabled`, `ImageAccess::BaseDir`).

## Images

The Markdown names a key (`![Figure](accepted/fig-3)`); your `ImageResolver` maps it to bytes you
chose, or answers "not mine". Nothing is read from disk and nothing is fetched for a key. A key that
looks like a URL or a path is still just a key. Unresolved keys fall back to `ImageAccess` (which
is `None` in `Config::default()`), so the image is replaced by its alt text and reported as
`image.disabled` / `image.remote`.

Whatever the source of the bytes, every raster is judged by the same pipeline: the header's claimed
size is checked **before** anything is allocated; the image is **fully decoded** (PNG included),
re-encoded as a plain 8-bit PNG (ancillary chunks, trailing data and malformed streams do not reach
Word) and charged to these budgets:

| Limit | Meaning |
| --- | --- |
| `max_image_bytes` | One source encoding |
| `max_total_image_bytes` | All distinct source encodings together |
| `max_distinct_images` | Number of distinct source encodings (an exact repeat of the same bytes counts once) |
| `max_image_pixels`, `max_image_dimension` | One image's area and longest side, from its header |
| `max_total_decode_pixels` | **Actual decode work**: every distinct encoding is charged when decoded, even if it normalises to pixels already seen. Only an exact byte-for-byte repeat reuses the cached result |
| `max_total_normalized_bytes` | The PNGs that end up in the document, each distinct picture once |
| `max_decode_alloc_bytes` | Memory one decode may allocate |
| `max_output_bytes` | The finished `.docx` |

Pictures carry accessible descriptions: the Markdown alt text and title become `wp:docPr/@descr` and
`@title`; a diagram or formula picture is described by its (shortened) source unless the author gave
`alt="…"` on the fence (```` ```mermaid alt="Login flow" ````). Every drawing has its own `wp:docPr` id.

## Findings and the strict policy

See [diagnostics.md](diagnostics.md). In short: `convert` returns the document and its
`Diagnostic`s; `analyze` returns the same findings without building the archive; with
`LossPolicy::Reject` any warning (a dropped image or link, a diagram left as code, an unknown LaTeX
command, lost footnote content, flattened list depth, unreferenced footnotes…) is
`CrateError::LossyContent` listing everything, with spans into your original text. The default is
`LossPolicy::Allow`, which is the historical behaviour: degrade as documented and report. Choose
explicitly.

## Scratch storage, cancellation, deadlines

* `Config::scratch_dir` is the *root*; each conversion that needs files makes one `silvercity-…`
  subdirectory (mode 0700 on Unix) in it, containing tool inputs, tool outputs and tool logs, and
  removes it on every normal return and on unwinding. No environment variable is changed.
  Renderer results from a `FenceRenderer` (including the in-process Mermaid renderer) and bytes from
  an `ImageResolver` are kept in memory and never touch the directory.
* After a worker is killed from outside (OOM killer, timeout supervisor), destructors do not run.
  `sweep_scratch(root, older_than)` removes leftover directories this crate made (name prefix, marker
  file, real directory, old enough). Even simpler: give every job its own root and delete it.
* `CancelToken` (any thread) and `Limits::max_conversion_time` are checked between Markdown events,
  between renders, before the archive is built, and every 10 ms while an external tool runs, in which case
  the tool's whole process group is killed first. They are **cooperative**: a synchronous callback
  that does not return is not interrupted. `RenderContext` (passed to `FenceRenderer::render_with`)
  exposes the deadline, the token and the size ceilings so a callback can stop early, and the
  in-process Mermaid renderer bridges them into merman's own cancellation. For a *hard* deadline
  run the conversion in a worker you can terminate from outside.

## Diagrams and a one-process worker

The default Mermaid engine is the `merman-cli` program, which means a child process. A worker that is
allowed exactly one process (a Windows job object with an active-process limit of 1; a Unix
supervisor that cannot reliably terminate children that start their own process group) should use the
in-process renderer instead:

```toml
silvercity-oss = { version = "0.1", default-features = false, features = ["mermaid"] }
```

```rust
let cfg = Config::default().with_in_process_mermaid();
```

It is a `FenceRenderer`, so it starts nothing and writes nothing, shares the conversion's deadline
and `CancelToken`, and is bounded by merman's resource policy (`MermaidProfile`) plus this crate's
image limits (a too-large raster is scaled down, not refused). The feature needs **Rust 1.95**
(merman's MSRV); the crate without it still builds on 1.88. merman is pinned to exactly the tested
version (`silvercity_oss::mermaid::MERMAN_VERSION`) with PNG export and eleven common diagram
families (flowchart, sequence, class, state, ER, pie, Gantt, journey, mindmap, timeline, git
graph); `mermaid-all-diagrams` adds the rest. It is distinct from anything parser-only: only this
renderer draws pictures. Text is measured with merman's deterministic metrics (no system fonts).

## What the library checks, and what needs the process around it

| Checked by the library (tested) | Needs the host |
| --- | --- |
| Input, element, image, output, workspace and tool-time budgets; header-judged, fully-decoded images; XML-safe output; link-scheme allow-list; path containment and symlink checks where files are enabled; kill-the-process-tree on every exit after a tool starts; scratch removal on normal exit; cooperative cancellation and deadlines | A memory (address-space/RSS) limit, a CPU limit, a hard wall-clock kill, a file-descriptor limit, no network, a private filesystem, reaping processes after a SIGKILL of the worker (a cgroup or job object), enforcing "one process" if that is your policy, and recovering scratch after a forced kill |
| Peak memory is *bounded in proportion to the limits* (a document costs a few KiB per element up to `max_elements`) | The converter cannot cap the memory of its own process or stop a thread that is stuck inside a callback or a decoder |

On Windows a timed-out external tool is killed but its descendants are not (no job object is used by
the library); use the in-process renderer, or put the worker in a job object.

## Reproducibility

Relationship, footnote, picture, bookmark and paragraph identifiers restart for every conversion
and are kept per thread, so equal input gives equal identifiers and one conversion cannot observe
another's activity. The package records a creation and modification time; by default that is the
time of the conversion, so bytes differ between runs. Set one and the whole file is reproducible
(same input, same configuration, same crate version, same bytes), which suits caches keyed by
content and snapshot tests:

```rust
use std::time::{Duration, UNIX_EPOCH};
let cfg = Config::default().with_timestamp(UNIX_EPOCH + Duration::from_secs(1_700_000_000));
```

## Links

A relative link is text unless you allow it. For documents you control, `Config::links`
(`LinkOptions`) can make links to local files and folders, rewrite them to web addresses, and link
documents converted together to each other; for documents you do not control, give the converter a
`LinkResolver` that maps the links you accept to addresses you choose, without it ever touching the
file system. See [links.md](links.md).

## Streams, async and blocking

The conversion is synchronous and CPU-bound. In an async service run it where blocking is
allowed (`tokio::task::spawn_blocking`, a dedicated thread pool, or a worker process) and give it a
`CancelToken` and a deadline so abandoned requests stop:

```rust
let docx = tokio::task::spawn_blocking(move || convert_reader(body_reader, &cfg)).await??;
```

`convert_reader` / `analyze_reader` take anything that implements `Read` (a request body, a pipe)
and read at most `Limits::max_input_bytes`; `convert_file` / `analyze_file` do the same for a path.
`Config` is `Send + Sync + Clone`: build it once per policy and share it.

If you would rather not link the library, the `md2docx` binary has a stable, scriptable interface:
standard input and output, a JSON report and a documented exit status ([cli.md](cli.md),
[containers.md](containers.md)).
