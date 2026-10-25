//! Stage 1: find math and graph blocks, render them, and rewrite the Markdown
//! so every one of them becomes an ordinary image reference.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

use crate::config::{Config, MathMode};
use crate::error::{CrateError, Result};
use crate::omml;
use crate::parser::markdown_options;
use crate::renderer::{asset_stem, render_graph, render_math_image, GraphKind, Workspace};
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
    /// A rendered PNG inside the pre-processor's workspace.
    Png(PathBuf),
    /// Native math: the body of an `<m:oMath>` element.
    Omml(String),
}

/// A pre-rendered asset, keyed by its image destination in the rewritten
/// Markdown.
#[derive(Debug, Clone)]
pub struct Asset {
    /// What the asset is.
    pub kind: AssetKind,
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
    _workspace: Workspace,
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].bytes().filter(|&b| b == b'\n').count() + 1
}

/// Escape a destination for `![..](<dest>)`. The asset map is keyed by the
/// *unescaped* string, which is what the Markdown parser hands back.
fn escape_dest(path: &str) -> String {
    path.replace('\\', "\\\\")
}

enum Found {
    Graph(GraphKind, String),
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
fn blank_front_matter(md: &str) -> std::borrow::Cow<'_, str> {
    let body = md.strip_prefix('\u{feff}').unwrap_or(md);
    let mut lines = body.split_inclusive('\n');
    let Some(first) = lines.next() else { return md.into() };
    let delim = first.trim_end();
    if delim != "---" && delim != "+++" {
        return md.into();
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
        consumed += line.len();
        let t = line.trim_end();
        if t == delim || (delim == "---" && t == "...") {
            let blanked: String = body[..consumed].chars().filter(|&c| c == '\n').collect();
            return format!("{blanked}{}", &body[consumed..]).into();
        }
        if !checked_first_content && !t.trim().is_empty() {
            checked_first_content = true;
            if !looks_like_metadata(t) {
                return md.into();
            }
        }
    }
    md.into() // never closed: not front matter
}

/// Run Stage 1 over `markdown`.
pub fn preprocess(markdown: &str, cfg: &Config) -> Result<Preprocessed> {
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
    // Control characters are illegal in XML; left in, Word calls the file corrupt.
    let markdown = sanitize_xml_chars(markdown);
    let markdown = &*blank_front_matter(&markdown);
    let too_many = || {
        CrateError::LimitExceeded(format!(
            "more than {} diagrams and formulas in one document",
            limits.max_rendered_blocks
        ))
    };

    // Pass 1: locate everything to replace, using real CommonMark parsing so
    // code spans, escapes and nested containers are handled correctly.
    let mut found: Vec<(Range<usize>, Found)> = Vec::new();
    let mut open_graph: Option<(Range<usize>, GraphKind, String)> = None;
    for (event, range) in Parser::new_ext(markdown, markdown_options(true)).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                // With diagrams disabled the fence stays an ordinary code block.
                if let Some(kind) = GraphKind::from_info(&info)
                    .filter(|_| cfg.graphs.enabled)
                    // Hardened mode renders Mermaid only; Dolphin is an arbitrary command.
                    .filter(|k| !(cfg.graphs.hardened && *k == GraphKind::Dolphin))
                {
                    open_graph = Some((range, kind, String::new()));
                }
            }
            Event::Text(t) => {
                if let Some((_, _, buf)) = open_graph.as_mut() {
                    buf.push_str(&t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((range, kind, buf)) = open_graph.take() {
                    if buf.trim().is_empty() {
                        return Err(CrateError::InvalidMarkdown(format!(
                            "empty {kind:?} block at line {}",
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
                    found.push((range, Found::Graph(kind, buf)));
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
        let diagrams = found.iter().filter(|(_, f)| matches!(f, Found::Graph(..))).count();
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
    let workspace = Workspace::with_budget(Some(limits.max_total_tool_time))?;
    let nonce = asset_stem("", &[markdown.as_bytes()]);
    let mut assets = HashMap::new();
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut omml_count = 0usize;

    for (range, item) in found {
        let (dest, alt, asset) = match item {
            Found::Graph(kind, source) => {
                let png = render_graph(&workspace, kind, &source, cfg)?;
                let dest = png.to_string_lossy().into_owned();
                (
                    dest,
                    "Rendered Graph",
                    Asset {
                        kind: AssetKind::Graph,
                        source: AssetSource::Png(png),
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
                        let png = render_math_image(&workspace, &latex, display, cfg)?;
                        let dest = png.to_string_lossy().into_owned();
                        (
                            dest,
                            "Rendered Math",
                            Asset {
                                kind,
                                source: AssetSource::Png(png),
                                original: latex,
                            },
                        )
                    }
                    MathMode::NativeOmml => {
                        let body = omml::latex_to_omml(&latex).map_err(|e| {
                            CrateError::InvalidMarkdown(format!(
                                "bad math at line {}: {e}",
                                line_of(markdown, range.start)
                            ))
                        })?;
                        let dest = format!("silvercity-omml{nonce}:{omml_count}");
                        omml_count += 1;
                        (
                            dest,
                            "Rendered Math",
                            Asset {
                                kind,
                                source: AssetSource::Omml(body),
                                original: latex,
                            },
                        )
                    }
                }
            }
        };
        replacements.push((range, format!("![{alt}](<{}>)", escape_dest(&dest))));
        assets.insert(dest, asset);
    }

    let mut out = markdown.to_string();
    replacements.sort_by_key(|r| std::cmp::Reverse(r.0.start));
    for (range, text) in replacements {
        out.replace_range(range, &text);
    }

    Ok(Preprocessed {
        markdown: out,
        assets,
        _workspace: workspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Tag;

    #[test]
    fn front_matter_only_at_the_start_and_only_if_it_looks_like_metadata() {
        let blank = |s: &str| blank_front_matter(s).into_owned();
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
