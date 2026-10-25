//! Stage 1: find math and graph blocks, render them, and rewrite the Markdown
//! so every one of them becomes an ordinary image reference.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

use crate::config::{Config, DiagramFailure, MathMode};
use crate::control::Ctl;
use crate::diagnostics::{Code, Collector, Edit, Loc, OffsetMap};
use crate::error::{CrateError, Result};
use crate::omml;
use crate::parser::markdown_options;
use crate::renderer::{asset_stem, render_custom, render_graph, render_math_image, GraphKind, Workspace};
use crate::safety::sanitize_xml_chars;

/// What an image reference in the rewritten Markdown stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// A rendered ```` ```mermaid ````/```` ```dolphin ```` diagram.
    Graph,
    /// A `$...$` formula.
    InlineMath,
    /// A `$$...$$` formula.
    DisplayMath,
}

/// Where a pre-rendered asset's content lives.
#[derive(Debug, Clone)]
pub enum AssetSource {
    /// A PNG a tool wrote inside the pre-processor's workspace.
    Png(PathBuf),
    /// A PNG held in memory: a [`FenceRenderer`](crate::FenceRenderer)'s result. Never touches
    /// the disk.
    Bytes(Arc<[u8]>),
    /// Native math: the body of an `<m:oMath>` element.
    Omml(String),
}

/// A pre-rendered asset, keyed by its image destination in the rewritten
/// Markdown.
#[derive(Debug, Clone)]
pub struct Asset {
    /// What the asset is.
    pub kind: AssetKind,
    /// Alternative text the author gave in the fence's info string (`alt="..."`), if any.
    pub alt: Option<String>,
    /// Title the author gave in the fence's info string (`title="..."`), if any.
    pub title: Option<String>,
    /// Its rendered content.
    pub source: AssetSource,
    /// Original graph/LaTeX text, used for the image's description.
    pub original: String,
}

/// Output of Stage 1. Owns the workspace: the rendered PNGs exist exactly as
/// long as this value does.
#[derive(Debug)]
pub struct Preprocessed {
    /// Markdown where math/graph blocks have been replaced by `![alt](dest)`.
    pub markdown: String,
    /// Image destination (as written in `markdown`) -> asset.
    pub assets: HashMap<String, Asset>,
    /// Title and author from the document's front matter.
    pub(crate) meta: (Option<String>, Option<String>),
    /// Cancellation and deadline of this conversion, shared with the later stages.
    pub(crate) ctl: Ctl,
    /// What Stage 1 found that the conversion could not carry over.
    pub(crate) diags: Collector,
    /// How to map positions in `markdown` back to the text the caller passed in.
    pub(crate) offsets: OffsetMap,
    _workspace: Option<Workspace>,
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].bytes().filter(|&b| b == b'\n').count() + 1
}

/// Escape a destination for `![..](<dest>)`. The asset map is keyed by the
/// *unescaped* string, which is what the Markdown parser hands back.
fn escape_dest(path: &str) -> String {
    path.replace('\\', "\\\\")
}

/// `alt="..."` and `title="..."` after the language on a fence's info string. They describe the
/// picture the block becomes; they never select a renderer or a path.
#[derive(Debug, Clone, Default)]
struct FenceAttrs {
    alt: Option<String>,
    title: Option<String>,
}

const MAX_FENCE_ATTR_CHARS: usize = 500;

fn fence_attrs(info: &str) -> FenceAttrs {
    let mut attrs = FenceAttrs::default();
    // Skip the language word, then read key="value" / key='value' pairs.
    let mut rest = info.trim_start();
    rest = rest.split_once(char::is_whitespace).map_or("", |(_, r)| r);
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().rsplit(char::is_whitespace).next().unwrap_or("");
        let after = &rest[eq + 1..];
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = after;
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(quote) else { break };
        let value: String = body[..end].chars().take(MAX_FENCE_ATTR_CHARS).collect();
        match key.to_ascii_lowercase().as_str() {
            "alt" if attrs.alt.is_none() => attrs.alt = Some(value),
            "title" if attrs.title.is_none() => attrs.title = Some(value),
            _ => {}
        }
        rest = &body[end + 1..];
    }
    attrs
}

/// Who renders a fenced block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fence {
    /// `mermaid` / `dolphin` through their configured commands.
    Builtin(GraphKind),
    /// A [`FenceRenderer`](crate::FenceRenderer) registered for this language.
    Custom(String),
}

enum Found {
    Graph(Fence, String, FenceAttrs),
    Math { latex: String, display: bool },
}

/// Blank out YAML (`---`) or TOML (`+++`) front matter at the very start of the
/// document, keeping line numbers intact.
///
/// This is done here rather than by the Markdown parser, which would also
/// swallow `---`/`+++` blocks in the middle of a document, deleting real
/// content that sits between two horizontal rules. To avoid eating a document
/// that merely starts with a rule, the block must look like metadata: empty,
/// or its first non-blank line is `key: value` (YAML) / `key = value` (TOML).
fn blank_front_matter(md: &str) -> (std::borrow::Cow<'_, str>, Option<Edit>) {
    let body = md.strip_prefix('\u{feff}').unwrap_or(md);
    match front_matter(body) {
        Some(fm) => {
            let blanked: String = body[..fm.consumed].chars().filter(|&c| c == '\n').collect();
            let edit = Edit {
                old: 0..(md.len() - body.len() + fm.consumed),
                new_len: blanked.len(),
            };
            (format!("{blanked}{}", &body[fm.consumed..]).into(), Some(edit))
        }
        None => (md.into(), None),
    }
}

/// Where `sanitize_xml_chars` removed characters from `text`: one edit per run of removed ones.
fn removed_characters(text: &str) -> Vec<Edit> {
    let mut edits: Vec<Edit> = Vec::new();
    for (i, c) in text.char_indices() {
        if crate::safety::is_xml_char(c) {
            continue;
        }
        match edits.last_mut() {
            Some(last) if last.old.end == i => last.old.end = i + c.len_utf8(),
            _ => edits.push(Edit {
                old: i..i + c.len_utf8(),
                new_len: 0,
            }),
        }
    }
    edits
}

/// A metadata block found at the very start of a document.
struct FrontMatter<'a> {
    /// `---` (YAML) or `+++` (TOML).
    delim: &'a str,
    /// The lines between the delimiters.
    inner: &'a str,
    /// Bytes of the whole block, closing delimiter line included.
    consumed: usize,
}

fn front_matter(body: &str) -> Option<FrontMatter<'_>> {
    let mut lines = body.split_inclusive('\n');
    let first = lines.next()?;
    let delim = first.trim_end();
    if delim != "---" && delim != "+++" {
        return None;
    }
    let looks_like_metadata = |line: &str| {
        let sep = if delim == "---" { ':' } else { '=' };
        line.split_once(sep).is_some_and(|(key, _)| {
            let key = key.trim();
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        })
    };
    let mut consumed = first.len();
    let mut checked_first_content = false;
    for line in lines {
        let t = line.trim_end();
        if t == delim || (delim == "---" && t == "...") {
            return Some(FrontMatter {
                delim,
                inner: &body[first.len()..consumed],
                consumed: consumed + line.len(),
            });
        }
        consumed += line.len();
        if !checked_first_content && !t.trim().is_empty() {
            checked_first_content = true;
            if !looks_like_metadata(t) {
                return None;
            }
        }
    }
    None // never closed: not front matter
}

/// Document title and author from simple `title:` / `author:` front matter lines
/// (`title = "x"` in TOML). Values are cut to 200 characters; lists and nested values are ignored.
pub(crate) fn front_matter_meta(md: &str) -> (Option<String>, Option<String>) {
    let body = md.strip_prefix('\u{feff}').unwrap_or(md);
    let Some(fm) = front_matter(body) else {
        return (None, None);
    };
    let sep = if fm.delim == "---" { ':' } else { '=' };
    let (mut title, mut author) = (None, None);
    for line in fm.inner.lines() {
        let Some((key, value)) = line.split_once(sep) else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        if value.is_empty() || value.starts_with(['[', '{', '|', '>']) {
            continue;
        }
        let value: String = sanitize_xml_chars(value)
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        match key.trim().to_lowercase().as_str() {
            "title" if title.is_none() => title = Some(value),
            "author" if author.is_none() => author = Some(value),
            _ => {}
        }
    }
    (title, author)
}

/// Run Stage 1 over `markdown`.
pub fn preprocess(markdown: &str, cfg: &Config) -> Result<Preprocessed> {
    preprocess_with(markdown, cfg, &Ctl::new(cfg))
}

/// Stage 1 under a conversion's cancellation and deadline.
pub(crate) fn preprocess_with(markdown: &str, cfg: &Config, ctl: &Ctl) -> Result<Preprocessed> {
    ctl.check()?;
    let limits = &cfg.limits;
    if markdown.len() > limits.max_input_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "input is {} bytes (limit {})",
            markdown.len(),
            limits.max_input_bytes
        )));
    }
    if markdown.contains('\0') {
        return Err(CrateError::InvalidMarkdown("input contains NUL bytes".into()));
    }
    let mut diags = Collector::default();
    let mut offsets = OffsetMap::default();
    // Control characters are illegal in XML; left in, Word calls the file corrupt.
    let sanitized = sanitize_xml_chars(markdown);
    if matches!(sanitized, std::borrow::Cow::Owned(_)) {
        offsets.sanitize = removed_characters(markdown);
        let removed: usize = offsets
            .sanitize
            .iter()
            .map(|e| markdown[e.old.clone()].chars().count())
            .sum();
        let first = offsets.sanitize.first().map(|e| e.old.clone());
        diags.push(
            Code::InputControlCharacters,
            format!("{removed} character(s) that cannot appear in a Word document were removed"),
            first.map_or(Loc::None, Loc::Original),
        );
    }
    let meta = front_matter_meta(&sanitized);
    let (blanked, front) = blank_front_matter(&sanitized);
    if let Some(edit) = front {
        diags.push(
            Code::InputFrontMatter,
            "the metadata block at the start was used for the title and author and is not rendered",
            Loc::Prepared(0..edit.new_len),
        );
        offsets.blank = vec![edit];
    }
    let markdown = &*blanked;
    let too_many = || {
        CrateError::LimitExceeded(format!(
            "more than {} diagrams and formulas in one document",
            limits.max_rendered_blocks
        ))
    };

    // Pass 1: locate everything to replace, using real CommonMark parsing so
    // code spans, escapes and nested containers are handled correctly.
    let mut found: Vec<(Range<usize>, Found)> = Vec::new();
    let mut open_graph: Option<(Range<usize>, Fence, String, FenceAttrs)> = None;
    let mut diagrams = 0usize;
    for (event, range) in Parser::new_ext(markdown, markdown_options(true)).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                // A renderer registered by the embedder wins, whatever the other settings.
                let custom = info
                    .split_whitespace()
                    .next()
                    .map(str::to_ascii_lowercase)
                    .filter(|lang| cfg.fence_renderers.get(lang).is_some());
                // Otherwise: with diagrams disabled the fence stays an ordinary code block.
                let fence = match custom {
                    Some(lang) => Some(Fence::Custom(lang)),
                    None => GraphKind::from_info(&info)
                        .filter(|_| cfg.graphs.enabled)
                        // Hardened mode renders Mermaid only; Dolphin is an arbitrary command.
                        .filter(|k| !(cfg.graphs.hardened && *k == GraphKind::Dolphin))
                        .map(Fence::Builtin),
                };
                if let Some(fence) = fence {
                    open_graph = Some((range, fence, String::new(), fence_attrs(&info)));
                } else if let Some(kind) = GraphKind::from_info(&info) {
                    // Left as an ordinary code block: the author asked for a picture.
                    let why = if !cfg.graphs.enabled {
                        "diagram rendering is disabled"
                    } else {
                        "this configuration only renders Mermaid"
                    };
                    diags.push(
                        Code::DiagramNotRendered,
                        format!("a {} block was left as a code block: {why}", kind.name()),
                        Loc::Prepared(range.clone()),
                    );
                }
            }
            Event::Text(t) => {
                if let Some((_, _, buf, _)) = open_graph.as_mut() {
                    buf.push_str(&t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((range, fence, buf, attrs)) = open_graph.take() {
                    if buf.trim().is_empty() {
                        return Err(CrateError::InvalidMarkdown(format!(
                            "empty {fence:?} block at line {}",
                            line_of(markdown, range.start)
                        )));
                    }
                    if buf.len() > limits.max_graph_source_bytes {
                        return Err(CrateError::LimitExceeded(format!(
                            "diagram at line {} is {} bytes (limit {})",
                            line_of(markdown, range.start),
                            buf.len(),
                            limits.max_graph_source_bytes
                        )));
                    }
                    diagrams += 1;
                    found.push((range, Found::Graph(fence, buf, attrs)));
                }
            }
            Event::InlineMath(m) | Event::DisplayMath(m) if m.len() > limits.max_formula_bytes => {
                return Err(CrateError::LimitExceeded(format!(
                    "formula at line {} is {} bytes (limit {})",
                    line_of(markdown, range.start),
                    m.len(),
                    limits.max_formula_bytes
                )));
            }
            Event::InlineMath(m) => found.push((
                range,
                Found::Math {
                    latex: m.trim().to_string(),
                    display: false,
                },
            )),
            Event::DisplayMath(m) => found.push((
                range,
                Found::Math {
                    latex: m.trim().to_string(),
                    display: true,
                },
            )),
            _ => {}
        }
        if diagrams > limits.max_diagrams {
            return Err(CrateError::LimitExceeded(format!(
                "more than {} diagrams in one document",
                limits.max_diagrams
            )));
        }
        if found.len() > limits.max_rendered_blocks {
            return Err(too_many());
        }
    }

    // Pass 2: render and build replacements.
    // The scratch directory exists only when something is rendered to a file: native math
    // and documents without diagrams need none, so a missing or read-only temp directory
    // does not break them.
    let needs_files = found.iter().any(|(_, f)| match f {
        // An embedder's renderer returns its PNG in memory and needs no scratch directory.
        Found::Graph(Fence::Custom(_), ..) => false,
        Found::Graph(..) => true,
        Found::Math { .. } => cfg.math.mode == MathMode::ImageFallback,
    });
    let workspace = if needs_files {
        Some(Workspace::create(cfg, ctl)?)
    } else {
        None
    };
    let scratch = || {
        workspace
            .as_ref()
            .ok_or_else(|| CrateError::DocxGenerationError("internal error: no scratch directory".into()))
    };
    let nonce = asset_stem("", &[markdown.as_bytes()]);
    let mut assets = HashMap::new();
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut omml_count = 0usize;

    let mut byte_assets = 0usize;
    // An identical block is rendered once: renderer output is held in memory, keyed by what asked.
    let mut rendered_once: HashMap<(String, String), (String, Arc<[u8]>)> = HashMap::new();
    for (range, item) in found {
        ctl.check()?;
        let is_fence = matches!(item, Found::Graph(..));
        let (dest, alt, asset) = match item {
            Found::Graph(fence, source, attrs) => {
                let rendered: Result<(String, AssetSource)> = match &fence {
                    Fence::Builtin(kind) => render_graph(scratch()?, *kind, &source, cfg)
                        .map(|png| (png.to_string_lossy().into_owned(), AssetSource::Png(png))),
                    Fence::Custom(lang) => {
                        let key = (lang.clone(), source.clone());
                        if let Some((dest, bytes)) = rendered_once.get(&key) {
                            Ok((dest.clone(), AssetSource::Bytes(Arc::clone(bytes))))
                        } else {
                            render_custom(ctl, lang, &source, cfg).map(|bytes| {
                                let dest = format!("silvercity-asset{nonce}:{byte_assets}");
                                byte_assets += 1;
                                let bytes: Arc<[u8]> = bytes.into();
                                rendered_once.insert(key, (dest.clone(), Arc::clone(&bytes)));
                                (dest, AssetSource::Bytes(bytes))
                            })
                        }
                    }
                };
                let (dest, source_of) = match rendered {
                    Ok(done) => done,
                    // The block stays what it was written as: an ordinary code block.
                    Err(e @ (CrateError::ExternalToolFailed { .. } | CrateError::MissingExternalCLI { .. }))
                        if cfg.graphs.on_failure == DiagramFailure::KeepAsCode =>
                    {
                        diags.push(
                            Code::DiagramKeptAsCode,
                            format!("the diagram could not be rendered ({e}) and was kept as a code block"),
                            Loc::Prepared(range.clone()),
                        );
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                (
                    dest,
                    "Rendered Graph",
                    Asset {
                        kind: AssetKind::Graph,
                        alt: attrs.alt,
                        title: attrs.title,
                        source: source_of,
                        original: source,
                    },
                )
            }
            Found::Math { latex, display } => {
                if latex.is_empty() {
                    return Err(CrateError::InvalidMarkdown(format!(
                        "empty math formula at line {}",
                        line_of(markdown, range.start)
                    )));
                }
                let kind = if display {
                    AssetKind::DisplayMath
                } else {
                    AssetKind::InlineMath
                };
                match cfg.math.mode {
                    MathMode::ImageFallback => {
                        let png = render_math_image(scratch()?, &latex, display, cfg)?;
                        let dest = png.to_string_lossy().into_owned();
                        (
                            dest,
                            "Rendered Math",
                            Asset {
                                kind,
                                alt: None,
                                title: None,
                                source: AssetSource::Png(png),
                                original: latex,
                            },
                        )
                    }
                    MathMode::NativeOmml => {
                        let (body, unknown) = omml::latex_to_omml_checked(&latex).map_err(|e| {
                            CrateError::InvalidMarkdown(format!(
                                "bad math at line {}: {e}",
                                line_of(markdown, range.start)
                            ))
                        })?;
                        if !unknown.is_empty() {
                            diags.push(
                                Code::MathUnknownCommand,
                                format!(
                                    "unsupported LaTeX command(s) {} are shown as literal text",
                                    unknown.iter().map(|c| format!("\\{c}")).collect::<Vec<_>>().join(", ")
                                ),
                                Loc::Prepared(range.clone()),
                            );
                        }
                        let dest = format!("silvercity-omml{nonce}:{omml_count}");
                        omml_count += 1;
                        (
                            dest,
                            "Rendered Math",
                            Asset {
                                kind,
                                alt: None,
                                title: None,
                                source: AssetSource::Omml(body),
                                original: latex,
                            },
                        )
                    }
                }
            }
        };
        let mut text = format!("![{alt}](<{}>)", escape_dest(&dest));
        if is_fence {
            // The line after a fence would otherwise continue the picture's paragraph. End it
            // with a blank line that carries the same quote markers.
            let line_start = markdown[..range.start].rfind('\n').map_or(0, |i| i + 1);
            let blank: String = markdown[line_start..range.start]
                .chars()
                .map(|c| if c == '>' { '>' } else { ' ' })
                .collect();
            text.push('\n');
            text.push_str(blank.trim_end());
            // The source's own newline completes the blank line, unless the range took it.
            if markdown[range.clone()].ends_with('\n') {
                text.push('\n');
            }
        }
        replacements.push((range, text));
        assets.insert(dest, asset);
    }

    let mut out = markdown.to_string();
    replacements.sort_by_key(|r| std::cmp::Reverse(r.0.start));
    offsets.rewrite = replacements
        .iter()
        .rev()
        .map(|(range, text)| Edit {
            old: range.clone(),
            new_len: text.len(),
        })
        .collect();
    for (range, text) in replacements {
        out.replace_range(range, &text);
    }

    Ok(Preprocessed {
        markdown: out,
        assets,
        meta,
        ctl: ctl.clone(),
        diags,
        offsets,
        _workspace: workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Tag;

    #[test]
    fn front_matter_only_at_the_start_and_only_if_it_looks_like_metadata() {
        let blank = |s: &str| blank_front_matter(s).0.into_owned();
        assert_eq!(blank("---\ntitle: x\ntags: [a]\n---\nBody\n"), "\n\n\n\nBody\n");
        assert_eq!(blank("+++\ntitle = \"x\"\n+++\nBody"), "\n\n\nBody");
        assert_eq!(blank("\u{feff}---\na: b\n---\nBody"), "\n\n\nBody");
        assert_eq!(blank("---\r\na: b\r\n...\r\nBody"), "\n\n\nBody");
        assert_eq!(blank("---\n---\nBody"), "\n\nBody", "empty block");
        // Content, not metadata: left alone.
        for keep in [
            "---\nJust a paragraph between rules\n---\nBody",
            "Intro\n\n---\nkey: value\n---\nAfter",
            "Intro\n\n+++\nkey = 1\n+++\nAfter",
            "---\nkey: value\nnever closed",
            "---",
            "",
            "text\n---\nkey: v\n---\n",
        ] {
            assert_eq!(blank(keep), keep, "{keep:?}");
        }
    }

    /// What the parser reports as the image destination for an escaped path.
    fn parsed_dest(escaped: &str) -> String {
        let md = format!("![x](<{escaped}>)");
        Parser::new(&md)
            .find_map(|e| match e {
                Event::Start(Tag::Image { dest_url, .. }) => Some(dest_url.to_string()),
                _ => None,
            })
            .expect("an image")
    }

    #[test]
    fn fence_attributes_are_read_after_the_language_only() {
        let a = fence_attrs("mermaid alt=\"Login flow\" title='The \"happy\" path'");
        assert_eq!(a.alt.as_deref(), Some("Login flow"));
        assert_eq!(a.title.as_deref(), Some("The \"happy\" path"));
        assert!(fence_attrs("mermaid").alt.is_none());
        assert!(fence_attrs("alt=\"x\"").alt.is_none(), "the first word is the language");
        assert!(fence_attrs("mermaid alt=unquoted").alt.is_none());
        assert!(fence_attrs("mermaid alt=\"never closed").alt.is_none());
        assert_eq!(
            fence_attrs(&format!("m alt=\"{}\"", "a".repeat(2000)))
                .alt
                .unwrap()
                .len(),
            500
        );
    }

    #[test]
    fn escaped_destinations_round_trip_through_the_parser() {
        for path in [
            r"/tmp/silvercity-abc/graph_1.png",
            r"C:\Users\me\AppData\Local\Temp\silvercity-.x\graph_1.png",
            r"C:\temp\.hidden\_under\(paren)\graph.png",
            "/tmp/with space/graph.png",
        ] {
            assert_eq!(parsed_dest(&escape_dest(path)), path);
        }
    }
}
