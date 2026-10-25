pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\n' => escaped.push_str("&#xA;"),
            // Escaping carriage returns causes errors in LibreOffice.
            '\r' => {}
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Escapes an owned XML value without reallocating plain text.
///
/// The original `String` is returned unchanged when it contains no character
/// that needs escaping or removal.
pub(crate) fn escape_owned(text: String) -> String {
    if text
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'&' | b'<' | b'>' | b'"' | b'\'' | b'\n' | b'\r'))
    {
        escape(&text)
    } else {
        text
    }
}
