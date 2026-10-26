# Links to files, folders and other documents

By default a link must be an absolute address with an allowed scheme (`https:`, `mailto:`). A
relative path such as `docs/cli.md` is shown as text and reported (`link.dropped`): a `.docx` has no
idea where it will be opened, and a link that points at the reader's disk is not something to
create from text you do not trust.

For documents **you** wrote, such as a README that links to `docs/`, `LICENSE` and a `tests/`
folder, this page describes how to turn those paths into links that work.

```sh
# The README, its linked documents, and the files they link to, as a self-contained tree
md2docx README.md --markdown-links follow --copy-linked -o site/

# One document whose links point at files where they are now
md2docx README.md --allow-links local

# One document for the web: links become GitHub addresses
md2docx README.md --allow-links local \
  --link-base https://github.com/me/repo/blob/v1.0/ --link-tree-base https://github.com/me/repo/tree/v1.0/

# A documentation check: every relative link and every `file.md#heading` must exist
md2docx --check --strict --allow-links local docs/*.md
```

## The model

| Term | Meaning | Set by |
| --- | --- | --- |
| **source directory** | The folder of the Markdown file; relative targets are resolved against it. | the input path (library: `LinkOptions::source_dir`) |
| **link root** | The tree links must stay inside. `/docs/x.md` means `docs/x.md` under the root, as on GitHub. | `--link-root` (default: the input's folder; the common folder of the inputs with `--markdown-links`) |
| **output directory** | The folder the `.docx` is saved in. Relative links are written *from here*. | `-o` (library: `LinkOptions::output_dir`) |

A link is resolved in this order: the host's `LinkResolver` (library only) → addresses with a
scheme (the existing allow-list) → local paths (below).

## 1. Local paths (`--allow-links none|local|all`)

| Mode | A path becomes a hyperlink when… | Otherwise |
| --- | --- | --- |
| `none` (default) | never | text, `link.dropped` |
| `local` | it names an **existing** file or directory **inside the link root** (after resolving symbolic links) and is not a program or macro document | text, with a finding that says why |
| `all` | always (also missing targets, targets outside the root, absolute paths, `file:` addresses, programs) | – |

`local` findings: `link.target-missing`, `link.outside-root`, `link.blocked` (an executable or
macro document such as `.exe`, `.bat`, `.ps1`, `.lnk`, `.docm`, or a network path). In `all` mode a
missing target is still reported (`link.target-missing`), but the link is made. **Network paths
(`\\host\share`, `//host/x`, `file://host/...`) are refused in every mode**: opening one sends the
reader's Windows credentials to that host.

`--strict` turns these findings into a failure, so `--check --strict --allow-links local` is a
link checker. For `file.md#heading` the heading is looked up in that file (`link.anchor-missing`;
switch off with `--no-check-link-anchors`).

### Where the link points: `--link-style rebased|absolute`

* `rebased` (default): a path relative to the **output directory**. `docs/cli.md` linked from
  `README.md` and saved to `build/README.docx` becomes `../docs/cli.md`. Moving the whole tree keeps
  the links working; moving the `.docx` on its own breaks them.
* `absolute`: a `file:///…` address. Moving the `.docx` anywhere on this machine keeps the links;
  another machine breaks them.

Directories get a trailing `/`. For a target that is not a `.docx`, a `#fragment` is dropped: the
program that opens it would not honour it, and Word could mistake it for part of the name.

## 2. Local paths as web addresses (`--link-base`, `--link-tree-base`, `--link-map`)

When the Markdown also lives on the web, point the links there instead:

```sh
--link-base      https://github.com/me/repo/blob/v1.0/   # files:        docs/cli.md  -> …/blob/v1.0/docs/cli.md#frag
--link-tree-base https://github.com/me/repo/tree/v1.0/   # directories:  tests/       -> …/tree/v1.0/tests/
--link-map       docs/=https://docs.example.com/          # first match wins, before the bases
```

The target must still exist inside the root (checks apply as in `local`). Pin the address to a tag
rather than `main` so an old document keeps meaning what it meant. The resulting address has to pass
the scheme list (`https` by default).

## 3. Links between documents (`--markdown-links keep|docx|follow`)

* `keep` (default): a link to a Markdown file points at that file, which opens in whatever program
  your system associates with `.md`.
* `docx`: the link points at the `.docx` that file becomes. `docs/cli.md#exit-status` opens
  `cli.docx` **at that heading**. You convert those files too: list them on the command line.
* `follow`: as `docx`, and every Markdown file reached through a link is converted as well,
  transitively, once each (cycles are fine). They appear in the report with `"origin":"link"`.

Headings are found through a bookmark that every document of the set carries for each heading and
HTML `id`/`name` (named from the anchor text, so no document needs to read another).

Where the documents go:

| Flag | Effect |
| --- | --- |
| `-o DIR` | The output directory (created if needed). Without it the `.docx` files are written next to their Markdown files. |
| `--link-root DIR` | The top of the tree. Files outside it are not followed and not linked. |
| `--link-layout mirror\|flat` | `mirror` (default) keeps the folder structure (`docs/deep/more.md` → `DIR/docs/deep/more.docx`). `flat` puts everything in `DIR`; two sources with the same name are an error. |
| `--copy-linked` | Copy the other files and folders the documents link to into `DIR`, at the same relative path, and link to the copies, so the output tree is self-contained. Needs `-o DIR` and `mirror`. Folders are copied whole; symbolic links inside them are skipped. |
| `--max-linked-files N`, `--max-copy-bytes SIZE` | Limits on documents plus copied files (default 1000) and bytes copied (default 256M). A run that would exceed them stops with status 4. |

`--check` follows links and reports every document without writing anything.

Linked Markdown files that are *copied* (because they sit inside a copied folder) are copied as
Markdown; they are converted only when a document links to them directly.

## 4. Your own rule (`LinkResolver`, library)

```rust
use md2docx::{Config, LinkDecision, LinkRequest};

let cfg = Config::default().with_link_resolver(|req: &LinkRequest<'_>| match req.target {
    t if t.starts_with("wiki:") => LinkDecision::Url(format!("https://wiki.example.com/{}", &t[5..])),
    "glossary" => LinkDecision::Document { target: "glossary.docx".into(), heading: Some("terms".into()) },
    _ => LinkDecision::Default, // fall back to the built-in rules
});
```

The resolver sees the destination as written and answers `Url`, `Document`, `Heading` (this
document's own), `Text` (drop the link) or `Default`. It never has to touch the file system, so it
is also the safe way to give *untrusted* documents real links: map keys to addresses you control.
The built-in options above are configuration of the same machinery: `Config::links`
(`LinkOptions`), with `LocalLinks`, `LinkStyle` and `DocumentLayout`; `Report::local_links` lists the
files a document linked to.

## What this does not do, and what to check in Word

* It does not make links work for a `.docx` that is moved away from the files it links to (use
  `absolute` on one machine, a web address anywhere, or `--copy-linked` and move the whole tree).
* Opening a link to a program or macro document is refused in `local` mode; in `all` mode that is
  your decision.
* The behaviour of relative targets, directory links and `file.docx#bookmark` is Word's. Relative
  targets are resolved against the folder the document is opened from; if Word asks before
  following a link, that prompt is Word's own.
* The path handling is written for Windows as well (drive letters, `\\?\` paths) but is exercised
  by the automated tests on Linux only. Try your own tree on Windows once.
