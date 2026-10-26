//! What `md2docx-cli` tells the caller: human lines, and the JSON report (`--format json`).
//!
//! The JSON is written by hand (the crate has no serde) and is a stable interface: the field
//! names and meanings below are `"schema": 1`; fields may be *added* within a schema version,
//! never renamed, removed or re-typed.

use md2docx::{Diagnostic, Severity, Stats};

use super::exit::CliError;

/// `file:line:column: warning[code]: message`, the shape editors and CI understand.
pub fn describe(input: &str, d: &Diagnostic) -> String {
    let place = d
        .span
        .as_ref()
        .map_or_else(String::new, |s| format!(":{}:{}", s.line, s.column));
    format!("{input}{place}: {}[{}]: {}", d.severity, d.code, d.message)
}

/// What a successful run produced.
pub struct Success {
    pub diagnostics: Vec<Diagnostic>,
    pub stats: Stats,
    /// Files and directories the document links to (not part of the JSON report).
    pub local_links: Vec<md2docx::LocalLink>,
}

fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Valid JSON, but they end a line in JavaScript.
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn diagnostic(out: &mut String, d: &Diagnostic) {
    out.push_str("{\"code\":");
    quote(out, d.code.as_str());
    out.push_str(",\"severity\":");
    quote(
        out,
        match d.severity {
            Severity::Info => "info",
            _ => "warning",
        },
    );
    out.push_str(",\"message\":");
    quote(out, &d.message);
    out.push_str(",\"span\":");
    match &d.span {
        Some(s) => out.push_str(&format!(
            "{{\"start\":{},\"end\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
            s.start, s.end, s.line, s.column, s.end_line, s.end_column
        )),
        None => out.push_str("null"),
    }
    out.push('}');
}

fn diagnostics(out: &mut String, list: &[Diagnostic]) {
    out.push('[');
    for (i, d) in list.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        diagnostic(out, d);
    }
    out.push(']');
}

/// One line of JSON for one input.
///
/// ```text
/// {"schema":1,"input":"a.md","output":"a.docx","origin":"input","status":"ok","exit_code":0,"error":null,
///  "diagnostics":[{"code":"link.dropped","severity":"warning","message":"...",
///                  "span":{"start":0,"end":9,"line":1,"column":1,"end_line":1,"end_column":10}}],
///  "stats":{"headings":1,"tables":0,"pictures":0,"equations":0,"footnotes":0,"hyperlinks":2,
///           "code_blocks":0}}
/// ```
///
/// `origin` is `"input"` for a file you named and `"link"` for a Markdown file converted because a
/// document linked to it (`--markdown-links follow`). `input` is `"-"` for standard input and `null` when the failure came before any input was
/// looked at (a usage error); `output` is `"-"` for standard output and `null` with
/// `--check` or when nothing was written. `status` is `"ok"` or `"error"`. On error `error` is
/// `{"kind": ..., "message": ...}`, `stats` is `null`, and `diagnostics` holds the findings behind a
/// `--strict` refusal (otherwise it is empty).
pub fn json(input: Option<&str>, output: Option<&str>, via_link: bool, result: &Result<Success, CliError>) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\"schema\":1,\"input\":");
    match input {
        Some(i) => quote(&mut out, i),
        None => out.push_str("null"),
    }
    out.push_str(",\"output\":");
    match (output, result) {
        (Some(o), Ok(_)) => quote(&mut out, o),
        _ => out.push_str("null"),
    }
    out.push_str(if via_link {
        ",\"origin\":\"link\""
    } else {
        ",\"origin\":\"input\""
    });
    match result {
        Ok(ok) => {
            out.push_str(",\"status\":\"ok\",\"exit_code\":0,\"error\":null,\"diagnostics\":");
            diagnostics(&mut out, &ok.diagnostics);
            let s = &ok.stats;
            out.push_str(&format!(
                ",\"stats\":{{\"headings\":{},\"tables\":{},\"pictures\":{},\"equations\":{},\"footnotes\":{},\"hyperlinks\":{},\"code_blocks\":{}}}}}",
                s.headings, s.tables, s.pictures, s.equations, s.footnotes, s.hyperlinks, s.code_blocks
            ));
        }
        Err(e) => {
            out.push_str(&format!(
                ",\"status\":\"error\",\"exit_code\":{},\"error\":{{\"kind\":",
                e.exit.code()
            ));
            quote(&mut out, e.kind);
            out.push_str(",\"message\":");
            quote(&mut out, &e.message);
            out.push_str("},\"diagnostics\":");
            diagnostics(&mut out, &e.diagnostics);
            out.push_str(",\"stats\":null}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::exit::Exit;

    #[test]
    fn strings_are_escaped_for_json() {
        let mut s = String::new();
        quote(&mut s, "a\"b\\c\nd\te\u{1}f\u{2028}g\u{7f}é");
        assert_eq!(s, "\"a\\\"b\\\\c\\nd\\te\\u0001f\\u2028g\\u007fé\"");
    }

    #[test]
    fn an_error_report_has_the_documented_shape() {
        let e = CliError::new(Exit::Rejected, "limit_exceeded", "too \"big\"");
        let line = json(Some("-"), Some("out.docx"), false, &Err(e));
        assert_eq!(
            line,
            "{\"schema\":1,\"input\":\"-\",\"output\":null,\"origin\":\"input\",\"status\":\"error\",\"exit_code\":4,\
             \"error\":{\"kind\":\"limit_exceeded\",\"message\":\"too \\\"big\\\"\"},\"diagnostics\":[],\"stats\":null}"
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn a_success_report_has_the_documented_shape() {
        let ok = Success {
            diagnostics: vec![],
            stats: Stats::default(),
            local_links: vec![],
        };
        assert_eq!(
            json(Some("a.md"), None, false, &Ok(ok)),
            "{\"schema\":1,\"input\":\"a.md\",\"output\":null,\"origin\":\"input\",\"status\":\"ok\",\"exit_code\":0,\"error\":null,\
             \"diagnostics\":[],\"stats\":{\"headings\":0,\"tables\":0,\"pictures\":0,\"equations\":0,\
             \"footnotes\":0,\"hyperlinks\":0,\"code_blocks\":0}}"
        );
    }
}
