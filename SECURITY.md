# Security policy

## Supported versions

Security fixes go into the latest release.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem. Use GitHub's
private vulnerability reporting on this repository (Security tab, "Report a
vulnerability"). Include the Markdown input (or a minimal reproduction), the
configuration (CLI flags or `Config`), the operating system, and what you
expected versus what happened. We aim to acknowledge reports within a few days.

## Scope

In scope: anything that lets a crafted Markdown document

* read or write files outside the intended directories,
* run programs or make the converter run programs it was not configured to run,
* crash, hang or exhaust memory/disk of the converter beyond the documented
  limits,
* produce a `.docx` that is corrupt or that contains active links to local or
  handler targets,
* bypass `Config::untrusted()` / `md2docx --untrusted`.

Out of scope: vulnerabilities in the external tools themselves (TeX, Mermaid CLI,
Chrome) when you chose to run them on untrusted input, and settings you
explicitly loosened (`ImageAccess::Any`, image math mode on untrusted text).
See the README's Security section for the threat model and known limitations.

## Trust model: library checks versus host isolation

**Who controls what.** The Markdown is untrusted. Everything else is controlled by the
embedding program and is trusted: `Config` (including `base_dir`, command templates,
`scratch_dir`), `FenceRenderer`s, `ImageResolver`s and asset maps, `Highlighter`s and custom
HTML tag styles. A document can never select a renderer, a path, a command or a limit.

**`Config::default()` is the untrusted-input policy.** Native math, no diagram or TeX
process, no local files, no raw-HTML links, tight limits. TeX, external renderers and local
files each need an explicit, trusted opt-in (`Config::trusted()`, or setting the field).
`Config::native_math()` is a *trusted* preset: it only removes the TeX process.

**What the library enforces** (checked in-process, tested): input, element, image, output and
workspace budgets; a bounded, normalised image decode; XML-safe output; link scheme allowlists;
lexical and symlink-aware path containment where files are allowed; bounded external-tool
runtime, console output and scratch space, with the tool's process tree killed on every exit
path once it has been started.

**What it cannot guarantee.** A library cannot bound the memory of its own process, interrupt
a synchronous callback, or contain a vulnerability in a decoder or renderer it links.
Deadlines and cancellation are cooperative. A host converting hostile documents should still
run the conversion in a worker with an address-space/RSS limit, a CPU and wall-clock limit,
no network, a private scratch filesystem, and a supervisor that kills the worker from
outside; the library's checks reduce what that supervisor has to absorb, they do not replace it.


## Dependencies and advisories

`cargo audit` is part of CI and the lock file is clean of vulnerabilities. The optional `mermaid`
feature pulls in `merman`, whose font code depends on `rustybuzz` and `ttf-parser`; the RustSec
database marks both **unmaintained** (RUSTSEC-2026-0206, RUSTSEC-2026-0192; informational, not a
vulnerability, and `cargo audit` reports them as warnings). They are compiled only with the
`mermaid` feature. Re-evaluate on each merman release. The `.docx` writer is first-party code in
`src/docx/` and `zip` is version 9, so there are no advisory exceptions.

See `docs/embedding.md` for what a worker that converts hostile documents must still enforce
from the outside, and `docs/integration-test-plan.md` for what is and is not verified.
