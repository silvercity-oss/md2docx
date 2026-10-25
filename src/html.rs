//! A deliberately small HTML tokenizer for the raw HTML that shows up in
//! Markdown (`<br>`, `<sup>`, `<img>`, `<div align=center>` badges...).
//!
//! It never builds a DOM, never fetches anything, and never fails: malformed
//! input degrades to text. Comments and the contents of `<script>`/`<style>`
//! (and other non-visible elements) are dropped.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// Decoded character data.
    Text(String),
    Tag {
        /// Lower-case element name.
        name: String,
        closing: bool,
        /// Raw attribute text, for [`attr`].
        attrs: String,
    },
}

/// Elements whose content is never shown.
const HIDDEN: &[&str] = &[
    "script", "style", "head", "title", "template", "noscript", "iframe", "object", "embed",
];

/// The hidden element (`script`, `style`, ...) that `raw` opens without closing
/// again, if any. Inline HTML arrives one tag per event, so the text between
/// `<script>` and `</script>` is a separate event the caller must skip.
pub(crate) fn unclosed_hidden(raw: &str) -> Option<&'static str> {
    let lower = raw.to_ascii_lowercase();
    HIDDEN.iter().copied().find(|name| {
        let open = format!("<{name}");
        lower.match_indices(&open).any(|(at, _)| {
            matches!(
                lower.as_bytes().get(at + open.len()),
                Some(b' ' | b'>' | b'/' | b'\t' | b'\n')
            ) && !lower[at..].contains(&format!("</{name}"))
        })
    })
}

/// True when `raw` contains the closing tag of hidden element `name`.
pub(crate) fn closes_hidden(raw: &str, name: &str) -> bool {
    raw.to_ascii_lowercase().contains(&format!("</{name}"))
}

pub(crate) fn tokenize(html: &str) -> Vec<Piece> {
    let lower = html.to_ascii_lowercase(); // same byte offsets as `html`
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let flush = |text: &mut String, out: &mut Vec<Piece>| {
        if !text.is_empty() {
            out.push(Piece::Text(decode_entities(text)));
            text.clear();
        }
    };
    while i < html.len() {
        let rest = &html[i..];
        if !rest.starts_with('<') {
            let next = rest.find('<').unwrap_or(rest.len());
            text.push_str(&rest[..next]);
            i += next;
            continue;
        }
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(html.len(), |e| i + e + 3);
            continue;
        }
        let closing = rest[1..].starts_with('/');
        let name_start = i + 1 + usize::from(closing);
        let name: String = lower[name_start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // `<!DOCTYPE`, `<?xml`, `a < b`: not an element; keep declarations out, '<' as text.
            if rest.starts_with("<!") || rest.starts_with("<?") {
                i = rest.find('>').map_or(html.len(), |e| i + e + 1);
            } else {
                text.push('<');
                i += 1;
            }
            continue;
        }
        // Find the closing '>' outside quoted attribute values.
        let attrs_start = name_start + name.len();
        let mut quote: Option<char> = None;
        let mut end = None;
        for (k, c) in html[attrs_start..].char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(attrs_start + k);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            // Unterminated tag: treat the rest as text.
            text.push_str(rest);
            break;
        };
        flush(&mut text, &mut out);
        let attrs = html[attrs_start..end].trim_end_matches('/').trim().to_string();
        i = end + 1;
        if !closing && HIDDEN.contains(&name.as_str()) {
            let close = format!("</{name}");
            i = lower[i..].find(&close).map_or(html.len(), |e| {
                let from = i + e;
                html[from..].find('>').map_or(html.len(), |g| from + g + 1)
            });
            continue;
        }
        out.push(Piece::Tag { name, closing, attrs });
    }
    flush(&mut text, &mut out);
    out
}

/// Value of attribute `name` in a raw attribute string (quoted or bare).
pub(crate) fn attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let boundary = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric() && lower.as_bytes()[at - 1] != b'-';
        let after = attrs[from..].trim_start();
        if !boundary || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let v = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or(""),
            _ => value.split(|c: char| c.is_whitespace()).next().unwrap_or(""),
        };
        return Some(decode_entities(v));
    }
    None
}

/// Decode the entities that matter in practice: the five XML ones, `&nbsp;`,
/// and numeric references. Unknown entities are left as written.
pub(crate) fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest[1..].find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &rest[1..1 + semi];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => name
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, semi + 2))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, closing: bool, attrs: &str) -> Piece {
        Piece::Tag {
            name: name.into(),
            closing,
            attrs: attrs.into(),
        }
    }

    #[test]
    fn tags_text_and_entities() {
        assert_eq!(
            tokenize("<p align=\"center\">a &amp; b<br/>c</p>"),
            vec![
                tag("p", false, "align=\"center\""),
                Piece::Text("a & b".into()),
                tag("br", false, ""),
                Piece::Text("c".into()),
                tag("p", true, ""),
            ]
        );
    }

    #[test]
    fn hidden_content_comments_and_declarations_are_dropped() {
        let t = tokenize("<!DOCTYPE html><!-- note --><script>alert('<b>x</b>')</script>ok<STYLE>p{}</STYLE>!");
        assert_eq!(t, vec![Piece::Text("ok".into()), Piece::Text("!".into())]);
        // Unterminated hidden element swallows the rest, never panics.
        assert_eq!(tokenize("a<script>never closed <b>"), vec![Piece::Text("a".into())]);
    }

    #[test]
    fn hidden_elements_split_across_inline_events() {
        assert_eq!(unclosed_hidden("<script>"), Some("script"));
        assert_eq!(unclosed_hidden("<STYLE type=\"x\">"), Some("style"));
        assert_eq!(unclosed_hidden("<script>x</script>"), None);
        assert_eq!(unclosed_hidden("<scripted>"), None);
        assert_eq!(unclosed_hidden("<b>"), None);
        assert!(closes_hidden("</SCRIPT >", "script"));
        assert!(!closes_hidden("</b>", "script"));
    }

    #[test]
    fn malformed_input_degrades_to_text() {
        assert_eq!(tokenize("a < b and c > d"), vec![Piece::Text("a < b and c > d".into())]);
        assert_eq!(tokenize("<div"), vec![Piece::Text("<div".into())]);
        assert_eq!(tokenize("<"), vec![Piece::Text("<".into())]);
        assert!(tokenize("<>").iter().all(|p| matches!(p, Piece::Text(_))));
        // '>' inside a quoted attribute does not end the tag.
        assert_eq!(tokenize("<a title=\"x>y\">t</a>")[0], tag("a", false, "title=\"x>y\""));
        // Multi-byte text next to markup is safe.
        assert_eq!(tokenize("日本<b>語</b>é").len(), 5);
    }

    #[test]
    fn attribute_lookup() {
        let a = "src='a b.png' ALT=\"x &amp; y\" width=40 data-src=\"no\"";
        assert_eq!(attr(a, "src").as_deref(), Some("a b.png"));
        assert_eq!(attr(a, "alt").as_deref(), Some("x & y"));
        assert_eq!(attr(a, "width").as_deref(), Some("40"));
        assert_eq!(attr("data-src=\"no\"", "src"), None, "`data-src` is not `src`");
        assert_eq!(attr("src", "src"), None);
    }

    #[test]
    fn entities() {
        assert_eq!(
            decode_entities("&lt;&gt;&quot;&#65;&#x42;&nbsp;&bogus; &amp"),
            "<>\"AB\u{a0}&bogus; &amp"
        );
        assert_eq!(
            decode_entities("&#0;&#xD800;&#99999999999;"),
            "\0&#xD800;&#99999999999;"
        );
        assert_eq!(decode_entities("é&é;"), "é&é;");
    }
}
