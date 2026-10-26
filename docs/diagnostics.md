# Diagnostics


Every finding the converter reports ([`Diagnostic`](https://docs.rs/silvercity-oss/latest/silvercity_oss/struct.Diagnostic.html)) has a **stable code**, a **severity** and, where it belongs to a place, a **span** in the Markdown you passed in (byte offsets, 1-based line and column). The codes below never change meaning once released; new ones may be added, so match on them with a fallback.

* **Warning**: something was dropped, degraded or left unconverted. `LossPolicy::Reject` refuses a document that has any, and `CrateError::LossyContent` carries the whole list.
* **Info**: nothing was lost. Never rejected.

Get them with `convert` / `convert_file` (with the document), `analyze` (without building the archive), or from `CrateError::LossyContent`. The CLI prints them as `file:line:column: warning[code]: message` and `--strict` makes them fatal.

| Code | Severity | What happened | What to do |
| --- | --- | --- | --- |
| `image.remote` | warning | A remote or inline image was not embedded. | Embed the image through an `ImageResolver`/`AssetMap` (the key may be the URL itself), or accept the alt text. |
| `image.disabled` | warning | A local image was not embedded because image access is disabled. | Supply the bytes through an `ImageResolver`/`AssetMap`; for your own documents use `Config::trusted()`. |
| `image.unreadable` | warning | A local image file could not be read (missing, unreadable, not a regular file); the alt text is shown. | Fix the path, or supply the bytes through an `ImageResolver`/`AssetMap`. |
| `image.unsupported` | warning | An image could not be embedded (unsupported or corrupt). | Use PNG, JPEG or GIF; check the file is not truncated. |
| `footnote.nested` | warning | A footnote reference inside a footnote was left as text. | Do not reference footnotes from footnotes. |
| `footnote.undefined` | info | A footnote reference has no definition, so `[^label]` stays as written (nothing is lost; a regular expression such as `[^a-z]` is legitimate text). | Add the definition `[^label]: text`, or escape the bracket. |
| `footnote.unreferenced` | warning | A footnote definition is never referenced, so its content was dropped. | Reference it from the text, or delete the definition. |
| `link.dropped` | warning | A link destination is not allowed, so only its text was kept. | Use an absolute `https:`/`mailto:` link (or whatever `Config::allowed_link_schemes` allows). |
| `link.modified` | warning | A link target was shortened: a `mailto:` link keeps only the `subject`, `body`, `cc` and `bcc` fields and no fragment. | Remove the other fields from the link. |
| `link.target-missing` | warning | A relative link names a file or directory that does not exist (local links on). | Fix the path. In `--allow-links all` the link is made anyway. |
| `link.outside-root` | warning | A relative link leaves the link root, or is an absolute path, and was not linked. | Move the target inside the root, widen `--link-root`, or use `--allow-links all`. |
| `link.blocked` | warning | A relative link names a program, a macro document or a network location and was not linked. | Link to something else; `all` links programs, never network paths. |
| `link.anchor-missing` | warning | A link points at a heading anchor that does not exist. | Fix the heading anchor (GitHub-style slug of the heading text). |
| `link.html-dropped` | warning | A raw HTML link was not turned into a hyperlink. | Use Markdown link syntax, or set `Config::html_links` for trusted input. |
| `list.depth-clamped` | warning | List nesting deeper than the supported levels was flattened. | Flatten the list; Word numbering defines nine levels. |
| `table.dropped` | warning | A table was dropped. | Do not nest HTML tables more than eight deep. |
| `html.unsupported` | warning | A raw HTML element is unsupported and its content was dropped. | Remove the element; its content is never rendered. |
| `diagram.not-rendered` | warning | A diagram was left as a code block (rendering is disabled). | Register a renderer (`with_in_process_mermaid()`, `with_fence_renderer`) or enable diagrams for trusted input. |
| `diagram.kept-as-code` | warning | A diagram could not be rendered and was kept as a code block. | Fix the diagram text, or accept the code block. |
| `math.unknown-command` | warning | An equation uses a LaTeX command that is not supported. | Use a supported command (see the README list of supported LaTeX). |
| `input.control-characters` | warning | Characters that cannot appear in a document were removed. | Remove the control characters from the source. |
| `input.front-matter` | info | Front matter was read and not rendered. | Nothing to do: the title and author were read from it. |
| `report.truncated` | warning | Further findings were not recorded. | Fix the earlier findings; at most 500 are kept. |

## Spans

A span is the smallest range of the **original** text that produced the finding. The converter changes the text before parsing it (it removes control characters, blanks front matter, and replaces diagram fences and `$` formulas by picture references), and every position is mapped back through those edits, so a span always indexes the string you gave to `convert`. When a construct was rewritten, the span is the whole original construct: a `mermaid` fence, or `$…$` with its dollar signs. `end` is exclusive; `end_line`/`end_column` are one past the last character.

## Keeping your own preview consistent

`analyze` runs the same pipeline as `convert` (same limits, same rendering of diagrams and formulas) and returns the same findings and `Stats` without compressing or keeping the archive, so a host that shows its own preview or validation can compare against it before committing to a conversion. `Stats` counts what the document contains (headings, tables, pictures, equations, footnotes, hyperlinks, code blocks) for cross-checks.
