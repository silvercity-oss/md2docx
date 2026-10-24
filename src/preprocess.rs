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

/// What an image reference in the rewritten Markdown stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Graph,
    InlineMath,
    DisplayMath,
}

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
    pub kind: AssetKind,
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

fn escape_dest(path: &str) -> String {
    path.replace('\\', "\\\\")
}

enum Found {
    Graph(GraphKind, String),
    Math { latex: String, display: bool },
}

/// Run Stage 1 over `markdown`.
pub fn preprocess(markdown: &str, cfg: &Config) -> Result<Preprocessed> {
    if markdown.contains('\0') {
        return Err(CrateError::InvalidMarkdown("input contains NUL bytes".into()));
    }

    // Pass 1: locate everything to replace, using real CommonMark parsing so
    // code spans, escapes and nested containers are handled correctly.
    let mut found: Vec<(Range<usize>, Found)> = Vec::new();
    let mut open_graph: Option<(Range<usize>, GraphKind, String)> = None;
    for (event, range) in Parser::new_ext(markdown, markdown_options(true)).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                if let Some(kind) = GraphKind::from_info(&info) {
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
                    found.push((range, Found::Graph(kind, buf)));
                }
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
    }

    // Pass 2: render and build replacements.
    let workspace = Workspace::new()?;
    let nonce = asset_stem("", &[markdown.as_bytes()]);
    let mut assets = HashMap::new();
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut omml_count = 0usize;

    for (range, item) in found {
        let (dest, alt, asset) = match item {
            Found::Graph(kind, source) => {
                let png = render_graph(&workspace, kind, &source, cfg)?;
                let dest = escape_dest(&png.to_string_lossy());
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
                        let dest = escape_dest(&png.to_string_lossy());
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
                        let dest = format!("reqmech-omml{nonce}:{omml_count}");
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
        replacements.push((range, format!("![{alt}](<{dest}>)")));
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
