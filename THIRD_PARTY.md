# Third-party material

* **docx-rs.** The `.docx` writer in `src/docx/` is a first-party, trimmed and modified fork of
  the writing half of docx-rs by bokuweb and contributors
  (<https://github.com/bokuweb/docx-rs>, MIT). The full licence text is in `LICENSE`. It is
  maintained in this repository and no longer a dependency.

* **merman** (optional `mermaid` feature only, not compiled by default). The in-process Mermaid
  renderer depends on `merman` 0.9.x (tested with 0.9.2; MIT OR Apache-2.0, <https://github.com/Latias94/merman>)
  with the ELK layout and math features turned off; its transitive dependencies (resvg, usvg,
  tiny-skia, ...) carry their own licences, listed in its `THIRD_PARTY_NOTICES.md` (all permissive, apart from
  a few MPL-2.0 crates such as `cssparser` and `selectors`, which are used unmodified; `deny.toml`
  allows exactly that set).

* **Theme colours.** `Theme::vscode_light()` and `Theme::vscode_dark()` use the token colours
  of Visual Studio Code's default *Light+* and *Dark+* themes
  (<https://github.com/microsoft/vscode>), distributed under the MIT licence:

  > Copyright (c) 2015 - present Microsoft Corporation
  >
  > Permission is hereby granted, free of charge, to any person obtaining a copy of this software
  > and associated documentation files (the "Software"), to deal in the Software without
  > restriction, including without limitation the rights to use, copy, modify, merge, publish,
  > distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
  > Software is furnished to do so, subject to the following conditions: The above copyright notice
  > and this permission notice shall be included in all copies or substantial portions of the
  > Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
  > INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
  > PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR
  > ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
  > ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
  > SOFTWARE.

* **Tests.** The repository's test data includes the inputs of the CommonMark specification's
  examples (CC BY-SA 4.0, see `tests/data/README.md`). They are not part of the published crate.
