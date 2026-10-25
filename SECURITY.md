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

## Known advisories in dependencies

`cargo audit` reports three advisories against `zip` 8.6.0 (RUSTSEC-2026-0367, -0368, -0369).
They concern extracting archives to disk (`extract()`: symlink targets, SUID/SGID bits). This
crate never extracts: it writes an archive through `docx-rs` and reads it back from memory by
index to recompress it. `docx-rs` (0.4.22, the latest release) pins `zip` 8, so the fixed
`zip` 9 cannot be used yet. The three IDs are ignored in `.cargo/audit.toml` with this
reasoning; the exception should be removed as soon as `docx-rs` allows `zip` 9.
