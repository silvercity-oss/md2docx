//! Machine-readable descriptions: what a [`Config`] will do, what the findings are, and what a
//! [`Report`] holds. For programs (and AI agents) that drive the converter and want facts, not prose.
//!
//! Every function returns compact-or-pretty JSON text with a `"schema"` number; fields may be added
//! within a schema, never renamed or re-typed.

use serde_json::{json, Map, Value};

use crate::config::{Config, PageSize};
use crate::diagnostics::{Code, Report, Severity};
use crate::highlight::BuiltinHighlighter;

fn secs(d: std::time::Duration) -> Value {
    json!(d.as_secs_f64())
}

fn name(v: &impl std::fmt::Debug) -> String {
    format!("{v:?}")
}

impl Report {
    /// The findings and statistics as JSON: `{"diagnostics": [...], "stats": {...}}`, the same
    /// objects the command line writes in its `--format json` report.
    pub fn to_json(&self) -> String {
        let diagnostics: Vec<Value> = self
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "code": d.code.as_str(),
                    "severity": match d.severity { Severity::Info => "info", _ => "warning" },
                    "message": d.message,
                    "span": d.span.as_ref().map(|s| json!({
                        "start": s.start, "end": s.end, "line": s.line, "column": s.column,
                        "end_line": s.end_line, "end_column": s.end_column,
                    })),
                })
            })
            .collect();
        let s = &self.stats;
        json!({
            "diagnostics": diagnostics,
            "stats": {
                "headings": s.headings, "tables": s.tables, "pictures": s.pictures,
                "equations": s.equations, "footnotes": s.footnotes, "hyperlinks": s.hyperlinks,
                "code_blocks": s.code_blocks,
            },
        })
        .to_string()
    }
}

impl Code {
    /// Every diagnostic code as JSON: `[{"code", "severity", "summary"}, ...]`, for a program that
    /// wants to know what it may be told before it converts anything.
    pub fn catalog_json() -> String {
        let list: Vec<Value> = Code::ALL
            .iter()
            .map(|c| {
                json!({
                    "code": c.as_str(),
                    "severity": match c.severity() { Severity::Info => "info", _ => "warning" },
                    "summary": c.summary(),
                })
            })
            .collect();
        serde_json::to_string_pretty(&json!({ "schema": 1, "codes": list })).unwrap_or_default() + "\n"
    }
}

impl BuiltinHighlighter {
    /// The languages that have a built-in lexer, by the name to write after the opening fence (aliases such as `rs`, `py`,
    /// `sh` and `yml` are also understood; see [`BuiltinHighlighter::supports`]).
    pub const LANGUAGES: &'static [&'static str] = &[
        "rust",
        "python",
        "javascript",
        "typescript",
        "java",
        "c",
        "cpp",
        "csharp",
        "go",
        "kotlin",
        "swift",
        "ruby",
        "php",
        "sql",
        "bash",
        "json",
        "yaml",
        "toml",
        "html",
        "css",
        "diff",
    ];
}

impl Config {
    /// What this configuration will do, as pretty-printed JSON: the limits, the trust-relevant
    /// switches, the document look and the link rules. Descriptive, not a file you can load back
    /// (a `Config` also holds code: renderers, resolvers, callbacks; those appear as `true` when set).
    pub fn settings_json(&self) -> String {
        let l = &self.limits;
        let limits = json!({
            "max_input_bytes": l.max_input_bytes,
            "max_rendered_blocks": l.max_rendered_blocks,
            "max_diagrams": l.max_diagrams,
            "max_graph_source_bytes": l.max_graph_source_bytes,
            "max_highlight_bytes": l.max_highlight_bytes,
            "max_formula_bytes": l.max_formula_bytes,
            "max_image_bytes": l.max_image_bytes,
            "max_total_image_bytes": l.max_total_image_bytes,
            "max_distinct_images": l.max_distinct_images,
            "max_image_pixels": l.max_image_pixels,
            "max_image_dimension": l.max_image_dimension,
            "max_total_decode_pixels": l.max_total_decode_pixels,
            "max_total_normalized_bytes": l.max_total_normalized_bytes,
            "max_decode_alloc_bytes": l.max_decode_alloc_bytes,
            "max_workspace_bytes": l.max_workspace_bytes,
            "max_output_bytes": l.max_output_bytes,
            "max_elements": l.max_elements,
            "max_uncompressed_bytes": l.max_uncompressed_bytes,
            "max_total_tool_time_secs": secs(l.max_total_tool_time),
            "max_conversion_time_secs": l.max_conversion_time.map(secs),
        });
        let d = &self.document;
        let (page_w, page_h) = d.page.twips();
        let page = match d.page {
            PageSize::A4 => "a4".to_string(),
            PageSize::Letter => "letter".to_string(),
            PageSize::Legal => "legal".to_string(),
            _ => "custom".to_string(),
        };
        let document = json!({
            "page": page,
            "page_width_pt": f64::from(page_w) / 20.0,
            "page_height_pt": f64::from(page_h) / 20.0,
            "margin_pt": f64::from(d.margin_twips) / 20.0,
            "body_font": d.body_font,
            "body_size_pt": f64::from(d.body_size_half_points) / 2.0,
            "code_font": d.code_font,
            "heading_color": d.heading_color,
            "code_background": d.code_background,
            "link_color": d.link_color,
            "quote_bar_color": d.quote_bar_color,
            "code_blocks": name(&d.code_blocks).to_lowercase(),
            "page_numbers": d.page_numbers,
            "number_headings": d.number_headings,
            "table_of_contents": d.table_of_contents.as_ref().map(|t| json!({"title": t.title, "max_level": t.max_level})),
            "title": d.title,
            "author": d.author,
            "fixed_timestamp": d.timestamp.is_some(),
        });
        let k = &self.links;
        let links = json!({
            "local": name(&k.local),
            "style": name(&k.style),
            "root": k.root.as_ref().map(|p| p.display().to_string()),
            "base_url": k.base_url,
            "dir_base_url": k.dir_base_url,
            "rewrites": k.rewrites.len(),
            "check_anchors": k.check_anchors,
            "document_tree": k.documents.is_some(),
        });
        let mut root = Map::new();
        root.insert("schema".into(), json!(1));
        root.insert(
            "math".into(),
            json!({ "mode": name(&self.math.mode), "dpi": self.math.dpi }),
        );
        root.insert(
            "diagrams".into(),
            json!({
                "enabled": self.graphs.enabled,
                "hardened": self.graphs.hardened,
                "on_failure": name(&self.graphs.on_failure),
                "mermaid_command": self.graphs.mermaid.program,
                "dolphin_command": self.graphs.dolphin.program,
                "dpi": self.graphs.dpi,
                "fence_renderers": !self.fence_renderers.is_empty(),
            }),
        );
        root.insert("image_access".into(), json!(name(&self.image_access)));
        root.insert("image_encoding".into(), json!(name(&self.image_encoding)));
        root.insert("image_resolver".into(), json!(self.image_resolver.is_some()));
        root.insert(
            "base_dir".into(),
            json!(self.base_dir.as_ref().map(|p| p.display().to_string())),
        );
        root.insert("tool_timeout_secs".into(), secs(self.tool_timeout));
        root.insert("allowed_link_schemes".into(), json!(self.allowed_link_schemes));
        root.insert("html_links".into(), json!(self.html_links));
        root.insert("autolink_bare_urls".into(), json!(self.autolink_bare_urls));
        root.insert("superscript_subscript".into(), json!(self.superscript_subscript));
        root.insert("compression_level".into(), json!(self.compression_level));
        root.insert("loss_policy".into(), json!(name(&self.loss_policy)));
        root.insert("highlighting".into(), json!(self.highlighting.is_some()));
        root.insert("reference_style".into(), json!(self.reference_docx.is_some()));
        root.insert(
            "scratch_dir".into(),
            json!(self.scratch_dir.as_ref().map(|p| p.display().to_string())),
        );
        root.insert("cancel_token".into(), json!(self.cancel.is_some()));
        root.insert("link_resolver".into(), json!(self.link_resolver.is_some()));
        root.insert("limits".into(), limits);
        root.insert("document".into(), document);
        root.insert("links".into(), links);
        serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default() + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_language_has_a_lexer() {
        for lang in BuiltinHighlighter::LANGUAGES {
            assert!(BuiltinHighlighter::supports(lang), "{lang}");
        }
    }

    #[test]
    fn the_catalog_lists_every_code_once() {
        let v: Value = serde_json::from_str(&Code::catalog_json()).unwrap();
        let codes = v["codes"].as_array().unwrap();
        assert_eq!(codes.len(), Code::ALL.len());
        assert!(codes
            .iter()
            .any(|c| c["code"] == "image.remote" && c["severity"] == "warning"));
    }

    #[test]
    fn settings_say_what_the_presets_allow() {
        let untrusted: Value = serde_json::from_str(&Config::untrusted().settings_json()).unwrap();
        let trusted: Value = serde_json::from_str(&Config::trusted().settings_json()).unwrap();
        assert_eq!(untrusted["image_access"], "None");
        assert_eq!(trusted["image_access"], "BaseDir");
        assert!(untrusted["limits"]["max_input_bytes"].as_u64() < trusted["limits"]["max_input_bytes"].as_u64());
        assert_eq!(untrusted["diagrams"]["enabled"], false);
    }

    #[test]
    fn a_report_serialises_its_findings_and_counts() {
        let report = crate::analyze("[x](javascript:alert(1)) \n\n# H\n", &Config::default()).unwrap();
        let v: Value = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(v["stats"]["headings"], 1);
        assert_eq!(v["diagnostics"][0]["code"], "link.dropped");
        assert_eq!(v["diagnostics"][0]["span"]["line"], 1);
    }
}
