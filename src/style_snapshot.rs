//! A reusable, editable snapshot of the styles a conversion would use.
//!
//! `--reference-doc some.docx` styles the output from a Word file. [`StyleSnapshot`] is the same
//! thing as a JSON document: [`StyleSnapshot::from_docx`] (the CLI's `--extract-reference`) writes
//! out the styles that *would be used*, which are the reference's own plus the converter's
//! definitions for the styles the reference lacks, and [`StyleSnapshot::from_json`] /
//! [`Config::with_reference_style`](crate::Config::with_reference_style) (`--reference-style`) read
//! it back. The JSON is for people and for programs that want to see or change a look without Word.
//!
//! ```text
//! {
//!   "schema": 1,
//!   "generator": "md2docx 0.1.0",
//!   "source": "brand.docx",
//!   "preamble_xml": "<w:styles ...><w:docDefaults>...</w:docDefaults>",   // used on read
//!   "doc_defaults": { "font": "Calibri", "size_pt": 11, "space_after_pt": 6 },   // summary
//!   "styles": [
//!     { "id": "Heading1", "type": "paragraph", "name": "heading 1",
//!       "properties": { "font": "Calibri Light", "size_pt": 20, "bold": true, "color": "1F3864" },
//!       "xml": "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\">...</w:style>" }   // used on read
//!   ],
//!   "theme_xml": "<a:theme ...>...</a:theme>"   // or null
//! }
//! ```
//!
//! Only `preamble_xml`, each style's `xml` and `theme_xml` are read back; `doc_defaults` and
//! `properties` are a readable summary of them, rebuilt every time a snapshot is written. To change
//! a style, edit its `xml` (and add or remove entries of `styles`).

use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::error::{CrateError, Result};
use crate::template::{style_blocks, ReferenceDocx};

/// The version of the JSON layout. A change that old readers cannot ignore bumps it.
pub const STYLE_SCHEMA: u64 = 1;

/// Largest style JSON accepted.
const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;

/// The styles (and theme) a conversion would use, as an editable JSON document.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StyleSnapshot {
    reference: ReferenceDocx,
    source: Option<String>,
}

fn bad_style(why: impl std::fmt::Display) -> CrateError {
    CrateError::InvalidConfig(format!("reference style: {why}"))
}

impl StyleSnapshot {
    /// The styles a conversion with `config` writes: its reference (document or style), the
    /// converter's definitions for the styles that reference lacks, and `config`'s own document
    /// options (fonts, sizes). Found by converting an empty document, so it is exactly what the
    /// conversion uses.
    pub fn from_config(config: &Config) -> Result<Self> {
        let made = crate::convert("", config)?;
        let reference = ReferenceDocx::from_bytes(&made.docx)?;
        Ok(StyleSnapshot {
            reference,
            source: None,
        })
    }

    /// The styles a conversion would use with `docx` as its reference document (the default
    /// configuration otherwise).
    pub fn from_docx(docx: &[u8]) -> Result<Self> {
        Self::from_config(&Config::default().with_reference_docx(docx)?)
    }

    /// Read a snapshot written by [`StyleSnapshot::to_json`] (possibly edited).
    pub fn from_json(json: &str) -> Result<Self> {
        if json.len() > MAX_JSON_BYTES {
            return Err(bad_style(format!("larger than {MAX_JSON_BYTES} bytes")));
        }
        let value: Value = serde_json::from_str(json).map_err(|e| bad_style(format!("not valid JSON ({e})")))?;
        let object = value
            .as_object()
            .ok_or_else(|| bad_style("the top level is not an object"))?;
        match object.get("schema").and_then(Value::as_u64) {
            Some(STYLE_SCHEMA) => {}
            Some(other) => {
                return Err(bad_style(format!(
                    "schema {other} is newer than this build reads (it reads {STYLE_SCHEMA})"
                )))
            }
            None => return Err(bad_style("it has no numeric \"schema\"")),
        }
        let text = |key: &str| -> Result<&str> {
            object
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| bad_style(format!("\"{key}\" is missing or not a string")))
        };
        let mut styles = String::from(text("preamble_xml")?);
        let list = object
            .get("styles")
            .and_then(Value::as_array)
            .ok_or_else(|| bad_style("\"styles\" is missing or not an array"))?;
        for (n, style) in list.iter().enumerate() {
            let xml = style
                .get("xml")
                .and_then(Value::as_str)
                .ok_or_else(|| bad_style(format!("styles[{n}] has no \"xml\" string")))?;
            if !xml.trim_start().starts_with("<w:style ") {
                return Err(bad_style(format!("styles[{n}].xml is not a <w:style> element")));
            }
            styles.push_str(xml);
        }
        styles.push_str("</w:styles>");
        let theme = match object.get("theme_xml") {
            None | Some(Value::Null) => None,
            Some(Value::String(t)) => Some(t.clone().into_bytes()),
            Some(_) => return Err(bad_style("\"theme_xml\" must be a string or null")),
        };
        let reference = ReferenceDocx::from_parts(styles, theme).map_err(|e| match e {
            CrateError::InvalidConfig(m) => bad_style(m.trim_start_matches("reference document: ")),
            other => other,
        })?;
        Ok(StyleSnapshot {
            reference,
            source: object.get("source").and_then(Value::as_str).map(str::to_string),
        })
    }

    /// Note where the styles came from (written as `"source"`; informational).
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The ids of the styles in the snapshot, in order.
    pub fn style_ids(&self) -> Vec<String> {
        style_blocks(&self.reference.styles)
            .map(|blocks| blocks.into_iter().map(|(id, _)| id).collect())
            .unwrap_or_default()
    }

    /// The snapshot as pretty-printed JSON (see the module documentation for the layout).
    pub fn to_json(&self) -> String {
        let styles = &self.reference.styles;
        let blocks = style_blocks(styles).unwrap_or_default();
        let first = blocks.first().map_or_else(
            || styles.rfind("</w:styles>").unwrap_or(styles.len()),
            |(_, b)| offset(styles, b),
        );
        let preamble = &styles[..first];

        let mut root = Map::new();
        root.insert("schema".into(), json!(STYLE_SCHEMA));
        root.insert(
            "generator".into(),
            json!(format!("md2docx {}", env!("CARGO_PKG_VERSION"))),
        );
        root.insert(
            "source".into(),
            self.source.as_deref().map_or(Value::Null, |s| json!(s)),
        );
        root.insert("preamble_xml".into(), json!(preamble));
        root.insert(
            "doc_defaults".into(),
            preamble
                .find("<w:docDefaults")
                .and_then(|a| preamble[a..].find("</w:docDefaults>").map(|b| &preamble[a..a + b]))
                .map_or_else(|| json!({}), |defaults| Value::Object(properties(defaults))),
        );
        let list: Vec<Value> = blocks.iter().map(|(id, block)| describe(id, block)).collect();
        root.insert("styles".into(), Value::Array(list));
        root.insert(
            "theme_xml".into(),
            self.reference
                .theme
                .as_ref()
                .and_then(|t| String::from_utf8(t.clone()).ok())
                .map_or(Value::Null, Value::String),
        );
        serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default() + "\n"
    }

    /// The snapshot as a reference for [`Config::with_reference_style`](crate::Config::with_reference_style).
    pub(crate) fn into_reference(self) -> ReferenceDocx {
        self.reference
    }
}

/// Byte offset of `part` inside `whole` (`part` must be a sub-slice of it).
fn offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

// ------------------------------------------------------------------------- readable summary

/// One element start tag: its name and attributes.
struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, String)>,
    /// The element names it is inside, innermost last.
    parents: Vec<&'a str>,
}

impl Tag<'_> {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
    }
    fn parent(&self) -> &str {
        self.parents.last().copied().unwrap_or("")
    }
    /// Word's on/off flags: present and not explicitly false.
    fn on(&self) -> bool {
        !matches!(self.attr("w:val"), Some("0" | "false" | "off"))
    }
}

fn unescape(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The start tags of `xml` (a fragment this crate or Word wrote), with their attributes and the
/// elements they sit in. Not a general XML parser: it only has to read `<w:style>` blocks.
fn tags(xml: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let Some(end) = rest.find('>') else { break };
        let inner = &rest[..end];
        rest = &rest[end + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        if let Some(close) = inner.strip_prefix('/') {
            if stack.last().is_some_and(|top| *top == close.trim()) {
                stack.pop();
            }
            continue;
        }
        let self_closing = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attr_text) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let mut attrs = Vec::new();
        let mut a = attr_text;
        while let Some(eq) = a.find("=\"") {
            let key = a[..eq].trim();
            let after = &a[eq + 2..];
            let Some(q) = after.find('"') else { break };
            attrs.push((key, unescape(&after[..q])));
            a = &after[q + 1..];
        }
        out.push(Tag {
            name,
            attrs,
            parents: stack.clone(),
        });
        if !self_closing {
            stack.push(name);
        }
    }
    out
}

fn points(twips: &str) -> Option<f64> {
    twips.parse::<f64>().ok().map(|t| (t / 20.0 * 100.0).round() / 100.0)
}

/// The formatting a style (or the document defaults) sets, in plain units.
fn properties(xml: &str) -> Map<String, Value> {
    let mut p = Map::new();
    for t in tags(xml) {
        let in_run = t.parent() == "w:rPr";
        let in_para = t.parent() == "w:pPr";
        let mut put = |k: &str, v: Value| {
            p.entry(k.to_string()).or_insert(v);
        };
        match t.name {
            "w:rFonts" if in_run => {
                if let Some(f) = t
                    .attr("w:ascii")
                    .or_else(|| t.attr("w:hAnsi"))
                    .or_else(|| t.attr("w:asciiTheme"))
                {
                    put("font", json!(f));
                }
            }
            "w:sz" if in_run => {
                if let Some(h) = t.attr("w:val").and_then(|v| v.parse::<f64>().ok()) {
                    put("size_pt", json!(h / 2.0));
                }
            }
            "w:b" if in_run => put("bold", json!(t.on())),
            "w:i" if in_run => put("italic", json!(t.on())),
            "w:caps" if in_run => put("caps", json!(t.on())),
            "w:strike" if in_run => put("strike", json!(t.on())),
            "w:u" if in_run => put("underline", json!(t.attr("w:val").is_none_or(|v| v != "none"))),
            "w:color" if in_run => {
                if let Some(c) = t.attr("w:val") {
                    put("color", json!(c));
                }
            }
            "w:highlight" if in_run => {
                if let Some(c) = t.attr("w:val") {
                    put("highlight", json!(c));
                }
            }
            "w:vertAlign" if in_run => {
                if let Some(c) = t.attr("w:val") {
                    put("vertical_align", json!(c));
                }
            }
            "w:shd" => {
                if let Some(c) = t.attr("w:fill").filter(|c| *c != "auto") {
                    put("shading", json!(c));
                }
            }
            "w:jc" if in_para => {
                if let Some(a) = t.attr("w:val") {
                    put("align", json!(a));
                }
            }
            "w:spacing" if in_para => {
                if let Some(v) = t.attr("w:before").and_then(points) {
                    put("space_before_pt", json!(v));
                }
                if let Some(v) = t.attr("w:after").and_then(points) {
                    put("space_after_pt", json!(v));
                }
                if let Some(line) = t.attr("w:line").and_then(|v| v.parse::<f64>().ok()) {
                    if t.attr("w:lineRule").is_none_or(|r| r == "auto") {
                        put("line_spacing", json!((line / 240.0 * 100.0).round() / 100.0));
                    } else if let Some(v) = points(&line.to_string()) {
                        put("line_height_pt", json!(v));
                    }
                }
            }
            "w:ind" if in_para => {
                for (key, name) in [
                    ("w:left", "indent_left_pt"),
                    ("w:start", "indent_left_pt"),
                    ("w:right", "indent_right_pt"),
                    ("w:firstLine", "indent_first_line_pt"),
                    ("w:hanging", "indent_hanging_pt"),
                ] {
                    if let Some(v) = t.attr(key).and_then(points) {
                        put(name, json!(v));
                    }
                }
            }
            "w:keepNext" if in_para => put("keep_with_next", json!(t.on())),
            "w:keepLines" if in_para => put("keep_lines_together", json!(t.on())),
            "w:pageBreakBefore" if in_para => put("page_break_before", json!(t.on())),
            "w:outlineLvl" if in_para => {
                if let Some(l) = t.attr("w:val").and_then(|v| v.parse::<u64>().ok()) {
                    put("outline_level", json!(l + 1));
                }
            }
            "w:pBdr" if in_para => put("paragraph_borders", json!(true)),
            _ => {}
        }
    }
    p
}

/// `{id, type, name, based_on, next, default, properties, xml}` for one `<w:style>` block.
fn describe(id: &str, block: &str) -> Value {
    let all = tags(block);
    let head = all.first();
    let child = |name: &str| {
        all.iter()
            .find(|t| t.name == name && t.parent() == "w:style")
            .and_then(|t| t.attr("w:val"))
            .map(str::to_string)
    };
    let mut o = Map::new();
    o.insert("id".into(), json!(id));
    o.insert(
        "type".into(),
        head.and_then(|t| t.attr("w:type")).map_or(Value::Null, |v| json!(v)),
    );
    o.insert("name".into(), child("w:name").map_or(Value::Null, Value::String));
    o.insert("based_on".into(), child("w:basedOn").map_or(Value::Null, Value::String));
    o.insert("next".into(), child("w:next").map_or(Value::Null, Value::String));
    o.insert(
        "default".into(),
        json!(head
            .and_then(|t| t.attr("w:default"))
            .is_some_and(|v| v == "1" || v == "true")),
    );
    o.insert("properties".into(), Value::Object(properties(block)));
    o.insert("xml".into(), json!(block));
    Value::Object(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_snapshot_lists_the_converters_styles_with_readable_properties() {
        let snap = StyleSnapshot::from_config(&Config::default()).unwrap();
        let ids = snap.style_ids();
        for want in [
            "Normal",
            "Heading1",
            "Hyperlink",
            "InlineCode",
            "CodeBlock",
            "FootnoteReference",
        ] {
            assert!(ids.iter().any(|i| i == want), "{want} in {ids:?}");
        }
        let json: Value = serde_json::from_str(&snap.to_json()).unwrap();
        assert_eq!(json["schema"], 1);
        let h1 = json["styles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Heading1")
            .unwrap();
        assert_eq!(h1["properties"]["bold"], true);
        assert!(h1["properties"]["size_pt"].as_f64().unwrap() > 11.0);
        assert!(h1["xml"].as_str().unwrap().starts_with("<w:style "));
    }

    #[test]
    fn the_json_round_trips_to_the_same_styles() {
        let snap = StyleSnapshot::from_config(&Config::default()).unwrap();
        let again = StyleSnapshot::from_json(&snap.to_json()).unwrap();
        assert_eq!(snap.style_ids(), again.style_ids());
        assert_eq!(snap.to_json(), again.to_json());
    }

    #[test]
    fn bad_json_is_a_configuration_error_that_says_what_is_wrong() {
        for (json, needle) in [
            ("nope", "not valid JSON"),
            ("[]", "not an object"),
            ("{}", "schema"),
            ("{\"schema\":2}", "newer"),
            ("{\"schema\":1}", "preamble_xml"),
            ("{\"schema\":1,\"preamble_xml\":\"<w:styles>\"}", "styles"),
            ("{\"schema\":1,\"preamble_xml\":\"<w:styles>\",\"styles\":[{}]}", "xml"),
            (
                "{\"schema\":1,\"preamble_xml\":\"<w:styles>\",\"styles\":[{\"xml\":\"<p/>\"}]}",
                "<w:style>",
            ),
            (
                "{\"schema\":1,\"preamble_xml\":\"<w:styles><b>\",\"styles\":[]}",
                "well-formed",
            ),
        ] {
            let e = StyleSnapshot::from_json(json).unwrap_err().to_string();
            assert!(e.contains("reference style") && e.contains(needle), "{json}: {e}");
        }
    }

    #[test]
    fn the_summary_reads_word_units() {
        let block = "<w:style w:type=\"paragraph\" w:styleId=\"X\"><w:name w:val=\"x\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:before=\"240\" w:after=\"120\" w:line=\"360\" w:lineRule=\"auto\"/><w:ind w:left=\"720\" w:hanging=\"360\"/><w:jc w:val=\"center\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"Georgia\"/><w:b w:val=\"0\"/><w:i/><w:sz w:val=\"25\"/><w:color w:val=\"112233\"/></w:rPr></w:style>";
        let d = describe("X", block);
        let p = &d["properties"];
        assert_eq!(d["based_on"], "Normal");
        assert_eq!(p["space_before_pt"], 12.0);
        assert_eq!(p["space_after_pt"], 6.0);
        assert_eq!(p["line_spacing"], 1.5);
        assert_eq!(p["indent_left_pt"], 36.0);
        assert_eq!(p["indent_hanging_pt"], 18.0);
        assert_eq!(p["align"], "center");
        assert_eq!(p["outline_level"], 2);
        assert_eq!(p["font"], "Georgia");
        assert_eq!(p["bold"], false);
        assert_eq!(p["italic"], true);
        assert_eq!(p["size_pt"], 12.5);
        assert_eq!(p["color"], "112233");
    }
}
