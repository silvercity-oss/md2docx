//! A small, dependency-free LaTeX-math to OMML (Office Math Markup Language)
//! converter.
//!
//! [`latex_to_omml`] returns the *body* of an `<m:oMath>` element. It covers
//! the constructs that show up in technical documents: fractions, scripts,
//! roots, n-ary operators (sums, integrals, ...), `\left..\right` delimiters,
//! accents, matrices/`cases`/`aligned`, font commands, Greek letters and a
//! large symbol table. Unknown commands are kept visible as literal text
//! instead of being dropped; structural problems (unbalanced braces, missing
//! arguments, unclosed `\left`, unknown environments) are errors.

use std::fmt;

/// Why a LaTeX formula could not be converted to OMML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmmlError(String);

impl OmmlError {
    /// What went wrong, as shown by `Display`.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OmmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OmmlError {}

type Res<T> = std::result::Result<T, OmmlError>;
/// Optional (subscript, superscript) pair.
type Scripts = (Option<Vec<Node>>, Option<Vec<Node>>);

fn err<T>(msg: impl Into<String>) -> Res<T> {
    Err(OmmlError(msg.into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Style {
    nor: bool,
    scr: Option<&'static str>,
    sty: Option<&'static str>,
}

#[derive(Debug, Clone)]
enum Node {
    Run {
        text: String,
        style: Style,
    },
    Frac {
        num: Vec<Node>,
        den: Vec<Node>,
        bar: bool,
    },
    Script {
        base: Vec<Node>,
        sub: Option<Vec<Node>>,
        sup: Option<Vec<Node>>,
    },
    Rad {
        deg: Option<Vec<Node>>,
        body: Vec<Node>,
    },
    Nary {
        chr: char,
        sub: Option<Vec<Node>>,
        sup: Option<Vec<Node>>,
        body: Vec<Node>,
    },
    Delim {
        open: String,
        sep: Option<String>,
        close: String,
        items: Vec<Vec<Node>>,
    },
    Accent {
        chr: char,
        body: Vec<Node>,
    },
    Bar {
        top: bool,
        body: Vec<Node>,
    },
    LimLow {
        base: Vec<Node>,
        lim: Vec<Node>,
    },
    Matrix {
        rows: Vec<Vec<Vec<Node>>>,
    },
    EqArr {
        rows: Vec<Vec<Node>>,
    },
}

/// The OOXML math namespace bound to the `m:` prefix.
pub(crate) const OMML_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

/// Make sure the `<w:document>` root binds `m:` to [`OMML_NS`], touching
/// nothing else. Tolerates any attribute layout (line breaks, single quotes,
/// a bare `<w:document>`), is idempotent, and refuses to continue when `m:` is
/// already bound to a different namespace, since silently overriding it would
/// corrupt the document.
#[cfg(test)]
pub(crate) fn ensure_math_namespace(xml: &str) -> std::result::Result<String, OmmlError> {
    ensure_math_namespace_in(xml, "<w:document")
}

/// [`ensure_math_namespace`] for another part's root element (`"<w:footnotes"`).
pub(crate) fn ensure_math_namespace_in(xml: &str, root: &str) -> std::result::Result<String, OmmlError> {
    let mut from = 0;
    let start = loop {
        let Some(rel) = xml[from..].find(root) else {
            return err(format!("the part has no {root}> root element"));
        };
        let at = from + rel;
        match xml[at + root.len()..].chars().next() {
            Some(c) if c.is_whitespace() || c == '>' || c == '/' => break at,
            _ => from = at + root.len(), // e.g. <w:documentProtection
        }
    };
    // End of the start tag, skipping '>' inside quoted attribute values.
    let name_end = start + root.len();
    let mut quote: Option<char> = None;
    let mut tag_end = None;
    for (i, c) in xml[name_end..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                tag_end = Some(name_end + i);
                break;
            }
            _ => {}
        }
    }
    let Some(tag_end) = tag_end else {
        return err(format!("unterminated {root}> start tag"));
    };
    let attrs = &xml[name_end..tag_end];
    if let Some(pos) = attrs.find("xmlns:m=") {
        let value = attrs[pos + "xmlns:m=".len()..].trim_start_matches(['"', '\'']);
        let value = value.split(['"', '\'']).next().unwrap_or("");
        return if value == OMML_NS {
            Ok(xml.to_string())
        } else {
            err(format!("`m:` is already bound to `{value}` on the document root"))
        };
    }
    let mut out = String::with_capacity(xml.len() + OMML_NS.len() + 16);
    out.push_str(&xml[..name_end]);
    out.push_str(" xmlns:m=\"");
    out.push_str(OMML_NS);
    out.push('"');
    out.push_str(&xml[name_end..]);
    Ok(out)
}

/// Convert LaTeX math (without `$` delimiters) to the inner XML of `<m:oMath>`.
#[cfg_attr(not(feature = "internals"), allow(dead_code))]
pub fn latex_to_omml(latex: &str) -> std::result::Result<String, OmmlError> {
    latex_to_omml_checked(latex).map(|(xml, _)| xml)
}

/// Like [`latex_to_omml`], and also lists the commands the converter does not know, in order of
/// first use (at most 16, without repeats). An unknown command is not an error: it is kept in the
/// equation as visible literal text, and this list is how a caller learns that happened.
pub fn latex_to_omml_checked(latex: &str) -> std::result::Result<(String, Vec<String>), OmmlError> {
    if let Some(c) = latex.chars().find(|&c| !crate::safety::is_xml_char(c)) {
        return err(format!("character U+{:04X} cannot appear in a document", c as u32));
    }
    let mut p = Parser {
        s: latex.chars().collect(),
        i: 0,
        depth: 0,
        unknown: Vec::new(),
    };
    let nodes = p.seq(&Style::default(), false)?;
    if p.i < p.s.len() {
        return match p.peek() {
            Some('}') => err("unmatched `}`"),
            Some('&') => err("`&` outside of a matrix/aligned environment"),
            _ => match p.peek_command().as_deref() {
                Some("right") => err("`\\right` without matching `\\left`"),
                Some("middle") => err("`\\middle` without matching `\\left`"),
                Some("end") => err("`\\end` without matching `\\begin`"),
                _ => err("unexpected trailing input"),
            },
        };
    }
    let mut out = String::new();
    render_seq(&nodes, &mut out);
    Ok((out, p.unknown))
}

// ---------------------------------------------------------------- parsing

/// Deepest nesting of groups, scripts, fractions, environments... Real
/// formulas stay in the single digits; the limit exists so hostile input like
/// `{{{{...` cannot overflow the stack (an abort, not a catchable panic).
const MAX_DEPTH: usize = 128;

struct Parser {
    s: Vec<char>,
    i: usize,
    depth: usize,
    /// Commands that were not recognised (first 16, distinct).
    unknown: Vec<String>,
}

fn is_nary_body_stop(c: char) -> bool {
    matches!(c, '+' | '-' | '=' | '<' | '>' | ',' | ';')
}

const BODY_STOP_COMMANDS: &[&str] = &[
    "pm",
    "mp",
    "le",
    "leq",
    "ge",
    "geq",
    "ne",
    "neq",
    "approx",
    "equiv",
    "sim",
    "to",
    "rightarrow",
    "leftarrow",
    "Rightarrow",
    "Leftarrow",
    "Leftrightarrow",
    "leftrightarrow",
    "mapsto",
    "in",
    "notin",
    "subset",
    "subseteq",
    "quad",
    "qquad",
    "propto",
];

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    /// Name of the command at the cursor, without consuming it. `\\` yields
    /// `"\\"`; a lone trailing backslash yields `None`.
    fn peek_command(&self) -> Option<String> {
        if self.peek() != Some('\\') {
            return None;
        }
        let mut j = self.i + 1;
        let first = *self.s.get(j)?;
        if !first.is_ascii_alphabetic() {
            return Some(first.to_string());
        }
        let mut name = String::new();
        while let Some(&c) = self.s.get(j) {
            if c.is_ascii_alphabetic() {
                name.push(c);
                j += 1;
            } else {
                break;
            }
        }
        Some(name)
    }

    fn take_command(&mut self) -> Res<String> {
        match self.peek_command() {
            Some(name) => {
                self.i += 1 + name.chars().count();
                Ok(name)
            }
            None => err("dangling `\\` at end of formula"),
        }
    }

    fn expect(&mut self, c: char) -> Res<()> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            err(format!("expected `{c}`"))
        }
    }

    /// Parse nodes until a closing token (`}`, `&`, `\\`, `\right`, `\middle`,
    /// `\end`) or the end of input, without consuming it. With `body` set, also
    /// stop before a top-level relation/operator (used for n-ary operands).
    fn seq(&mut self, style: &Style, body: bool) -> Res<Vec<Node>> {
        self.seq_until(style, body, false)
    }

    /// Like [`Parser::seq`]; with `bracket` set it also stops before a `]`
    /// (the optional argument of `\sqrt[n]{x}`).
    fn seq_until(&mut self, style: &Style, body: bool, bracket: bool) -> Res<Vec<Node>> {
        if self.depth >= MAX_DEPTH {
            return err(format!("formula is nested more than {MAX_DEPTH} levels deep"));
        }
        self.depth += 1;
        let out = self.seq_inner(style, body, bracket);
        self.depth -= 1;
        out
    }

    fn seq_inner(&mut self, style: &Style, body: bool, bracket: bool) -> Res<Vec<Node>> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else { break };
            if c == '}' || c == '&' || (bracket && c == ']') {
                break;
            }
            let before = self.i;
            if c == '\\' {
                match self.peek_command().as_deref() {
                    Some("\\" | "right" | "middle" | "end") => break,
                    Some(name) if body && BODY_STOP_COMMANDS.contains(&name) => break,
                    _ => {}
                }
            } else if body && is_nary_body_stop(c) {
                break;
            }
            let mut base = self.atom(style)?;
            let (mut sub, mut sup) = (None, None);
            loop {
                self.skip_ws();
                match self.peek() {
                    Some('^') => {
                        self.i += 1;
                        if sup.is_some() {
                            return err("double superscript");
                        }
                        sup = Some(self.arg(style)?);
                    }
                    Some('_') => {
                        self.i += 1;
                        if sub.is_some() {
                            return err("double subscript");
                        }
                        sub = Some(self.arg(style)?);
                    }
                    Some('\'') => {
                        let mut primes = String::new();
                        while self.peek() == Some('\'') {
                            self.i += 1;
                            primes.push('\u{2032}');
                        }
                        if sup.is_some() {
                            return err("double superscript");
                        }
                        sup = Some(vec![Node::Run {
                            text: primes,
                            style: style.clone(),
                        }]);
                    }
                    _ => break,
                }
            }
            if sub.is_some() || sup.is_some() {
                base = vec![Node::Script { base, sub, sup }];
            }
            out.extend(base);
            // Every iteration must consume input; a parser bug must be an
            // error, never a hang.
            if self.i == before {
                return err("internal error: formula parser made no progress");
            }
        }
        Ok(out)
    }

    /// A required argument: `{group}` or a single atom.
    fn arg(&mut self, style: &Style) -> Res<Vec<Node>> {
        self.skip_ws();
        match self.peek() {
            None => err("missing argument at end of formula"),
            Some('}') => err("missing argument before `}`"),
            Some('{') => self.group(style),
            Some(_) => self.atom(style),
        }
    }

    fn group(&mut self, style: &Style) -> Res<Vec<Node>> {
        self.expect('{')?;
        let inner = self.seq(style, false)?;
        self.skip_ws();
        if self.peek() != Some('}') {
            return err("unclosed `{`");
        }
        self.i += 1;
        Ok(inner)
    }

    /// Raw text of a `{...}` group with nested braces balanced (for `\text`,
    /// environment names, column specs).
    fn raw_group(&mut self) -> Res<String> {
        self.skip_ws();
        if self.peek() != Some('{') {
            return err("expected `{`");
        }
        self.i += 1;
        let mut depth = 1usize;
        let mut text = String::new();
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(text);
                    }
                }
                _ => {}
            }
            text.push(c);
        }
        err("unclosed `{`")
    }

    fn run(&self, text: impl Into<String>, style: &Style) -> Node {
        Node::Run {
            text: text.into(),
            style: style.clone(),
        }
    }

    fn atom(&mut self, style: &Style) -> Res<Vec<Node>> {
        // Chains such as `\mathbf\mathbf\mathbf...` recurse through here without
        // ever entering `seq`, so atoms are depth-counted too.
        if self.depth >= MAX_DEPTH {
            return err(format!("formula is nested more than {MAX_DEPTH} levels deep"));
        }
        self.depth += 1;
        let out = self.atom_inner(style);
        self.depth -= 1;
        out
    }

    fn atom_inner(&mut self, style: &Style) -> Res<Vec<Node>> {
        let Some(c) = self.peek() else {
            return err("unexpected end of formula");
        };
        match c {
            '{' => self.group(style),
            '\\' => self.command(style),
            '^' | '_' => Ok(Vec::new()), // script on an empty base, e.g. `{}^{14}C`
            '$' => {
                self.i += 1;
                Ok(Vec::new())
            }
            '~' => {
                self.i += 1;
                Ok(vec![self.run(
                    "\u{00A0}",
                    &Style {
                        nor: true,
                        ..Style::default()
                    },
                )])
            }
            '-' => {
                self.i += 1;
                Ok(vec![self.run("\u{2212}", style)])
            }
            _ => {
                self.i += 1;
                Ok(vec![self.run(c.to_string(), style)])
            }
        }
    }

    fn sub_sup(&mut self, style: &Style) -> Res<Scripts> {
        let (mut sub, mut sup) = (None, None);
        loop {
            self.skip_ws();
            match self.peek() {
                Some('_') if sub.is_none() => {
                    self.i += 1;
                    sub = Some(self.arg(style)?);
                }
                Some('^') if sup.is_none() => {
                    self.i += 1;
                    sup = Some(self.arg(style)?);
                }
                _ => return Ok((sub, sup)),
            }
        }
    }

    fn command(&mut self, style: &Style) -> Res<Vec<Node>> {
        let name = self.take_command()?;
        let n = name.as_str();

        // Structures.
        match n {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.arg(style)?;
                let den = self.arg(style)?;
                return Ok(vec![Node::Frac { num, den, bar: true }]);
            }
            "binom" | "dbinom" | "tbinom" => {
                let num = self.arg(style)?;
                let den = self.arg(style)?;
                let f = Node::Frac { num, den, bar: false };
                return Ok(vec![Node::Delim {
                    open: "(".into(),
                    sep: None,
                    close: ")".into(),
                    items: vec![vec![f]],
                }]);
            }
            "sqrt" => {
                self.skip_ws();
                let deg = if self.peek() == Some('[') {
                    self.i += 1;
                    let d = self.seq_until_bracket(style)?;
                    Some(d)
                } else {
                    None
                };
                let body = self.arg(style)?;
                return Ok(vec![Node::Rad { deg, body }]);
            }
            "left" => return self.left_right(style),
            "begin" => return self.environment(style),
            "text" | "textrm" | "textit" | "textbf" | "mbox" | "hbox" | "operatorname" | "mathrm" => {
                let raw = self.raw_group()?;
                let st = if n == "operatorname" || n == "mathrm" {
                    Style {
                        sty: Some("p"),
                        ..style.clone()
                    }
                } else {
                    Style {
                        nor: true,
                        ..style.clone()
                    }
                };
                if n == "mathrm" {
                    // allow structure inside \mathrm{..}
                    let mut p = Parser {
                        s: raw.chars().collect(),
                        i: 0,
                        depth: self.depth,
                        unknown: Vec::new(),
                    };
                    let nodes = p.seq(&st, false);
                    for u in p.unknown {
                        if self.unknown.len() < 16 && !self.unknown.contains(&u) {
                            self.unknown.push(u);
                        }
                    }
                    return nodes;
                }
                return Ok(vec![self.run(raw, &st)]);
            }
            "mathbf" | "mathit" | "mathbb" | "mathcal" | "mathscr" | "mathfrak" | "mathsf" | "mathtt"
            | "boldsymbol" | "bm" | "mathnormal" => {
                let st = match n {
                    "mathbf" => Style {
                        sty: Some("b"),
                        ..style.clone()
                    },
                    "mathit" | "mathnormal" => Style {
                        sty: Some("i"),
                        ..style.clone()
                    },
                    "boldsymbol" | "bm" => Style {
                        sty: Some("bi"),
                        ..style.clone()
                    },
                    "mathbb" => Style {
                        scr: Some("double-struck"),
                        sty: Some("p"),
                        ..style.clone()
                    },
                    "mathcal" | "mathscr" => Style {
                        scr: Some("script"),
                        sty: Some("p"),
                        ..style.clone()
                    },
                    "mathfrak" => Style {
                        scr: Some("fraktur"),
                        sty: Some("p"),
                        ..style.clone()
                    },
                    "mathsf" => Style {
                        scr: Some("sans-serif"),
                        sty: Some("p"),
                        ..style.clone()
                    },
                    _ => Style {
                        scr: Some("monospace"),
                        sty: Some("p"),
                        ..style.clone()
                    },
                };
                return self.arg(&st);
            }
            "overline" => {
                let body = self.arg(style)?;
                return Ok(vec![Node::Bar { top: true, body }]);
            }
            "underline" => {
                let body = self.arg(style)?;
                return Ok(vec![Node::Bar { top: false, body }]);
            }
            _ => {}
        }

        if let Some(chr) = accent_char(n) {
            let body = self.arg(style)?;
            return Ok(vec![Node::Accent { chr, body }]);
        }

        if let Some(chr) = nary_char(n) {
            let (sub, sup) = self.sub_sup(style)?;
            let body = self.nary_body(style)?;
            return Ok(vec![Node::Nary { chr, sub, sup, body }]);
        }

        if FUNCTIONS.contains(&n) {
            let upright = Style {
                sty: Some("p"),
                ..style.clone()
            };
            let word = self.run(n, &upright);
            if LIMIT_FUNCTIONS.contains(&n) {
                self.skip_ws();
                if self.peek() == Some('_') {
                    self.i += 1;
                    let lim = self.arg(style)?;
                    return Ok(vec![Node::LimLow { base: vec![word], lim }]);
                }
            }
            return Ok(vec![word]);
        }

        if let Some(text) = spacing(n) {
            return Ok(vec![self.run(
                text,
                &Style {
                    nor: true,
                    ..Style::default()
                },
            )]);
        }

        if let Some(sym) = symbol(n) {
            return Ok(vec![self.run(sym, style)]);
        }

        // Escaped literals such as `\{`, `\%`, `\_`.
        if n.chars().count() == 1 && !n.chars().all(|c| c.is_ascii_alphabetic()) {
            return Ok(vec![self.run(n, style)]);
        }

        if matches!(
            n,
            "limits" | "nolimits" | "displaystyle" | "textstyle" | "scriptstyle" | "big" | "Big" | "bigg" | "Bigg"
        ) {
            return Ok(Vec::new());
        }

        // Unknown: keep it visible rather than silently dropping content.
        if self.unknown.len() < 16 && !self.unknown.iter().any(|u| u == n) {
            self.unknown.push(n.to_string());
        }
        Ok(vec![self.run(
            format!("\\{n}"),
            &Style {
                nor: true,
                ..Style::default()
            },
        )])
    }

    /// The `[n]` of `\sqrt[n]{x}`; the opening `[` has been consumed.
    fn seq_until_bracket(&mut self, style: &Style) -> Res<Vec<Node>> {
        let out = self.seq_until(style, false, true)?;
        self.skip_ws();
        if self.peek() != Some(']') {
            return err("unclosed `[` in \\sqrt");
        }
        self.i += 1;
        Ok(out)
    }

    fn nary_body(&mut self, style: &Style) -> Res<Vec<Node>> {
        self.seq(style, true)
    }

    fn delimiter(&mut self) -> Res<String> {
        self.skip_ws();
        match self.peek() {
            None => err("missing delimiter after \\left/\\right/\\middle"),
            Some('\\') => {
                let name = self.take_command()?;
                Ok(match name.as_str() {
                    "{" | "lbrace" | "}" | "rbrace" => (if name == "{" || name == "lbrace" { "{" } else { "}" }).into(),
                    "|" | "Vert" | "lVert" | "rVert" => "\u{2016}".into(),
                    "vert" | "lvert" | "rvert" => "|".into(),
                    "langle" => "\u{27E8}".into(),
                    "rangle" => "\u{27E9}".into(),
                    "lfloor" => "\u{230A}".into(),
                    "rfloor" => "\u{230B}".into(),
                    "lceil" => "\u{2308}".into(),
                    "rceil" => "\u{2309}".into(),
                    "backslash" => "\\".into(),
                    other => return err(format!("unsupported delimiter `\\{other}`")),
                })
            }
            Some('.') => {
                self.i += 1;
                Ok(String::new())
            }
            Some(c) => {
                self.i += 1;
                Ok(c.to_string())
            }
        }
    }

    fn left_right(&mut self, style: &Style) -> Res<Vec<Node>> {
        let open = self.delimiter()?;
        let mut items = Vec::new();
        let mut sep: Option<String> = None;
        loop {
            items.push(self.seq(style, false)?);
            match self.peek_command().as_deref() {
                Some("middle") => {
                    self.take_command()?;
                    sep = Some(self.delimiter()?);
                }
                Some("right") => {
                    self.take_command()?;
                    let close = self.delimiter()?;
                    return Ok(vec![Node::Delim {
                        open,
                        sep,
                        close,
                        items,
                    }]);
                }
                _ => return err("`\\left` without matching `\\right`"),
            }
        }
    }

    fn environment(&mut self, style: &Style) -> Res<Vec<Node>> {
        let name = self.raw_group()?;
        let name = name.trim().to_string();
        let kind = match name.as_str() {
            "matrix" | "smallmatrix" => ("", ""),
            "pmatrix" => ("(", ")"),
            "bmatrix" => ("[", "]"),
            "Bmatrix" => ("{", "}"),
            "vmatrix" => ("|", "|"),
            "Vmatrix" => ("\u{2016}", "\u{2016}"),
            "array" => ("", ""),
            "cases" => ("{", ""),
            "aligned" | "align" | "align*" | "alignat" | "split" | "gather" | "gather*" | "gathered" | "eqnarray"
            | "eqnarray*" | "equation" | "equation*" => ("", ""),
            other => return err(format!("unsupported environment `{other}`")),
        };
        if name == "array" {
            self.raw_group()?; // column spec
        }
        let mut rows: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut row: Vec<Vec<Node>> = Vec::new();
        loop {
            let cell = self.seq(style, false)?;
            self.skip_ws();
            match self.peek() {
                Some('&') => {
                    self.i += 1;
                    row.push(cell);
                }
                _ => match self.peek_command().as_deref() {
                    Some("\\") => {
                        self.take_command()?;
                        row.push(cell);
                        rows.push(std::mem::take(&mut row));
                    }
                    Some("end") => {
                        self.take_command()?;
                        let end = self.raw_group()?;
                        if end.trim() != name {
                            return err(format!("`\\begin{{{name}}}` closed by `\\end{{{end}}}`"));
                        }
                        if !(cell.is_empty() && row.is_empty()) {
                            row.push(cell);
                            rows.push(row);
                        }
                        break;
                    }
                    _ => return err(format!("unclosed `\\begin{{{name}}}`")),
                },
            }
        }
        if rows.is_empty() {
            return err(format!("empty `{name}` environment"));
        }
        let body = if name.contains("matrix") || name == "array" {
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            for r in &mut rows {
                r.resize_with(cols, Vec::new);
            }
            Node::Matrix { rows }
        } else {
            // cases / aligned / gather...: one equation-array line per row;
            // `&` alignment points collapse into a thin gap.
            let gap = self.run(
                "\u{2009}",
                &Style {
                    nor: true,
                    ..Style::default()
                },
            );
            let lines = rows
                .into_iter()
                .map(|cells| {
                    let mut line = Vec::new();
                    for (k, cell) in cells.into_iter().enumerate() {
                        if k > 0 {
                            line.push(gap.clone());
                        }
                        line.extend(cell);
                    }
                    line
                })
                .collect();
            Node::EqArr { rows: lines }
        };
        if kind.0.is_empty() && kind.1.is_empty() {
            Ok(vec![body])
        } else {
            Ok(vec![Node::Delim {
                open: kind.0.into(),
                sep: None,
                close: kind.1.into(),
                items: vec![vec![body]],
            }])
        }
    }
}

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "coth", "log",
    "ln", "lg", "exp", "lim", "liminf", "limsup", "max", "min", "sup", "inf", "det", "dim", "gcd", "ker", "deg", "arg",
    "Pr", "hom", "mod", "bmod",
];

const LIMIT_FUNCTIONS: &[&str] = &[
    "lim", "liminf", "limsup", "max", "min", "sup", "inf", "det", "gcd", "Pr",
];

fn nary_char(name: &str) -> Option<char> {
    Some(match name {
        "sum" => '\u{2211}',
        "prod" => '\u{220F}',
        "coprod" => '\u{2210}',
        "int" => '\u{222B}',
        "iint" => '\u{222C}',
        "iiint" => '\u{222D}',
        "oint" => '\u{222E}',
        "bigcup" => '\u{22C3}',
        "bigcap" => '\u{22C2}',
        "bigvee" => '\u{22C1}',
        "bigwedge" => '\u{22C0}',
        "bigoplus" => '\u{2A01}',
        "bigotimes" => '\u{2A02}',
        _ => return None,
    })
}

fn is_integral(c: char) -> bool {
    matches!(c, '\u{222B}'..='\u{222E}')
}

fn accent_char(name: &str) -> Option<char> {
    Some(match name {
        "hat" | "widehat" => '\u{0302}',
        "tilde" | "widetilde" => '\u{0303}',
        "bar" => '\u{0304}',
        "breve" => '\u{0306}',
        "dot" => '\u{0307}',
        "ddot" => '\u{0308}',
        "check" => '\u{030C}',
        "acute" => '\u{0301}',
        "grave" => '\u{0300}',
        "vec" | "overrightarrow" => '\u{20D7}',
        _ => return None,
    })
}

fn spacing(name: &str) -> Option<&'static str> {
    Some(match name {
        "," | "thinspace" => "\u{2009}",
        ":" | ">" | "medspace" => "\u{205F}",
        ";" | "thickspace" => "\u{2004}",
        " " => "\u{00A0}",
        "!" => "",
        "quad" => "\u{2003}",
        "qquad" => "\u{2003}\u{2003}",
        _ => return None,
    })
}

#[rustfmt::skip]
fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        // Greek
        "alpha" => "α", "beta" => "β", "gamma" => "γ", "delta" => "δ", "epsilon" => "ϵ", "varepsilon" => "ε",
        "zeta" => "ζ", "eta" => "η", "theta" => "θ", "vartheta" => "ϑ", "iota" => "ι", "kappa" => "κ",
        "lambda" => "λ", "mu" => "μ", "nu" => "ν", "xi" => "ξ", "omicron" => "ο", "pi" => "π", "varpi" => "ϖ",
        "rho" => "ρ", "varrho" => "ϱ", "sigma" => "σ", "varsigma" => "ς", "tau" => "τ", "upsilon" => "υ",
        "phi" => "ϕ", "varphi" => "φ", "chi" => "χ", "psi" => "ψ", "omega" => "ω",
        "Gamma" => "Γ", "Delta" => "Δ", "Theta" => "Θ", "Lambda" => "Λ", "Xi" => "Ξ", "Pi" => "Π",
        "Sigma" => "Σ", "Upsilon" => "Υ", "Phi" => "Φ", "Psi" => "Ψ", "Omega" => "Ω",
        // Operators and relations
        "times" => "×", "cdot" => "⋅", "div" => "÷", "pm" => "±", "mp" => "∓", "ast" => "∗", "star" => "⋆",
        "circ" => "∘", "bullet" => "∙", "oplus" => "⊕", "ominus" => "⊖", "otimes" => "⊗", "odot" => "⊙",
        "le" | "leq" => "≤", "ge" | "geq" => "≥", "ll" => "≪", "gg" => "≫", "ne" | "neq" => "≠",
        "approx" => "≈", "equiv" => "≡", "sim" => "∼", "simeq" => "≃", "cong" => "≅", "propto" => "∝",
        "in" => "∈", "notin" => "∉", "ni" => "∋", "subset" => "⊂", "supset" => "⊃", "subseteq" => "⊆",
        "supseteq" => "⊇", "cup" => "∪", "cap" => "∩", "setminus" => "∖", "emptyset" | "varnothing" => "∅",
        "forall" => "∀", "exists" => "∃", "nexists" => "∄", "neg" | "lnot" => "¬", "land" | "wedge" => "∧",
        "lor" | "vee" => "∨", "implies" => "⟹", "iff" => "⟺", "therefore" => "∴", "because" => "∵",
        "perp" => "⊥", "parallel" => "∥", "mid" => "∣", "angle" => "∠", "triangle" => "△",
        // Arrows
        "to" | "rightarrow" => "→", "leftarrow" | "gets" => "←", "leftrightarrow" => "↔", "Rightarrow" => "⇒",
        "Leftarrow" => "⇐", "Leftrightarrow" => "⇔", "mapsto" => "↦", "uparrow" => "↑", "downarrow" => "↓",
        "longrightarrow" => "⟶", "longleftarrow" => "⟵",
        // Misc
        "infty" => "∞", "partial" => "∂", "nabla" => "∇", "hbar" => "ℏ", "ell" => "ℓ", "aleph" => "ℵ",
        "Re" => "ℜ", "Im" => "ℑ", "wp" => "℘", "prime" => "′", "degree" => "°", "dots" | "ldots" => "…",
        "cdots" => "⋯", "vdots" => "⋮", "ddots" => "⋱", "square" => "□", "dagger" => "†", "ddagger" => "‡",
        "S" => "§", "P" => "¶", "copyright" => "©", "surd" => "√",
        "lbrace" => "{", "rbrace" => "}", "langle" => "⟨", "rangle" => "⟩", "lfloor" => "⌊", "rfloor" => "⌋",
        "lceil" => "⌈", "rceil" => "⌉", "vert" => "|", "Vert" => "‖", "backslash" => "\\",
        _ => return None,
    })
}

// -------------------------------------------------------------- rendering

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn val_elem(tag: &str, val: &str, out: &mut String) {
    out.push_str("<m:");
    out.push_str(tag);
    out.push_str(" m:val=\"");
    esc(val, out);
    out.push_str("\"/>");
}

fn wrap(tag: &str, nodes: &[Node], out: &mut String) {
    out.push_str("<m:");
    out.push_str(tag);
    out.push('>');
    render_seq(nodes, out);
    out.push_str("</m:");
    out.push_str(tag);
    out.push('>');
}

fn render_run(text: &str, style: &Style, out: &mut String) {
    out.push_str("<m:r>");
    if *style != Style::default() {
        out.push_str("<m:rPr>");
        if style.nor {
            out.push_str("<m:nor/>");
        }
        if let Some(scr) = style.scr {
            val_elem("scr", scr, out);
        }
        if let Some(sty) = style.sty {
            val_elem("sty", sty, out);
        }
        out.push_str("</m:rPr>");
    }
    if !style.nor {
        out.push_str("<w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr>");
    }
    out.push_str("<m:t xml:space=\"preserve\">");
    esc(text, out);
    out.push_str("</m:t></m:r>");
}

fn render_seq(nodes: &[Node], out: &mut String) {
    let mut k = 0;
    while k < nodes.len() {
        if let Node::Run { text, style } = &nodes[k] {
            // Merge adjacent runs of identical style into one `<m:r>`.
            let mut merged = text.clone();
            let mut j = k + 1;
            while let Some(Node::Run { text: t2, style: s2 }) = nodes.get(j) {
                if s2 != style {
                    break;
                }
                merged.push_str(t2);
                j += 1;
            }
            render_run(&merged, style, out);
            k = j;
        } else {
            render_node(&nodes[k], out);
            k += 1;
        }
    }
}

fn render_node(node: &Node, out: &mut String) {
    match node {
        Node::Run { text, style } => render_run(text, style, out),
        Node::Frac { num, den, bar } => {
            out.push_str("<m:f>");
            if !bar {
                out.push_str("<m:fPr>");
                val_elem("type", "noBar", out);
                out.push_str("</m:fPr>");
            }
            wrap("num", num, out);
            wrap("den", den, out);
            out.push_str("</m:f>");
        }
        Node::Script { base, sub, sup } => {
            let tag = match (sub, sup) {
                (Some(_), Some(_)) => "sSubSup",
                (Some(_), None) => "sSub",
                _ => "sSup",
            };
            out.push_str(&format!("<m:{tag}>"));
            wrap("e", base, out);
            if let Some(s) = sub {
                wrap("sub", s, out);
            }
            if let Some(s) = sup {
                wrap("sup", s, out);
            }
            out.push_str(&format!("</m:{tag}>"));
        }
        Node::Rad { deg, body } => {
            out.push_str("<m:rad>");
            match deg {
                Some(d) => wrap("deg", d, out),
                None => {
                    out.push_str("<m:radPr>");
                    val_elem("degHide", "1", out);
                    out.push_str("</m:radPr><m:deg></m:deg>");
                }
            }
            wrap("e", body, out);
            out.push_str("</m:rad>");
        }
        Node::Nary { chr, sub, sup, body } => {
            out.push_str("<m:nary><m:naryPr>");
            val_elem("chr", &chr.to_string(), out);
            val_elem("limLoc", if is_integral(*chr) { "subSup" } else { "undOvr" }, out);
            if sub.is_none() {
                val_elem("subHide", "1", out);
            }
            if sup.is_none() {
                val_elem("supHide", "1", out);
            }
            out.push_str("</m:naryPr>");
            wrap("sub", sub.as_deref().unwrap_or(&[]), out);
            wrap("sup", sup.as_deref().unwrap_or(&[]), out);
            wrap("e", body, out);
            out.push_str("</m:nary>");
        }
        Node::Delim {
            open,
            sep,
            close,
            items,
        } => {
            out.push_str("<m:d><m:dPr>");
            val_elem("begChr", open, out);
            if let Some(s) = sep {
                val_elem("sepChr", s, out);
            }
            val_elem("endChr", close, out);
            out.push_str("</m:dPr>");
            for item in items {
                wrap("e", item, out);
            }
            out.push_str("</m:d>");
        }
        Node::Accent { chr, body } => {
            out.push_str("<m:acc><m:accPr>");
            val_elem("chr", &chr.to_string(), out);
            out.push_str("</m:accPr>");
            wrap("e", body, out);
            out.push_str("</m:acc>");
        }
        Node::Bar { top, body } => {
            out.push_str("<m:bar><m:barPr>");
            val_elem("pos", if *top { "top" } else { "bot" }, out);
            out.push_str("</m:barPr>");
            wrap("e", body, out);
            out.push_str("</m:bar>");
        }
        Node::LimLow { base, lim } => {
            out.push_str("<m:limLow>");
            wrap("e", base, out);
            wrap("lim", lim, out);
            out.push_str("</m:limLow>");
        }
        Node::Matrix { rows } => {
            out.push_str("<m:m>");
            for row in rows {
                out.push_str("<m:mr>");
                for cell in row {
                    wrap("e", cell, out);
                }
                out.push_str("</m:mr>");
            }
            out.push_str("</m:m>");
        }
        Node::EqArr { rows } => {
            out.push_str("<m:eqArr>");
            for row in rows {
                wrap("e", row, out);
            }
            out.push_str("</m:eqArr>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(s: &str) -> String {
        latex_to_omml(s).unwrap()
    }

    const NS: &str = "xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"";

    #[test]
    fn namespace_is_added_to_the_root_only() {
        let xml = "<?xml version=\"1.0\"?>\n<w:document xmlns:w=\"urn:w\"\n   mc:Ignorable=\"w14\"><w:body><w:p/></w:body></w:document>";
        let out = ensure_math_namespace(xml).unwrap();
        assert_eq!(out.matches(NS).count(), 1);
        assert!(out.contains(&format!("<w:document {NS} xmlns:w=")));
        assert!(out.ends_with("<w:body><w:p/></w:body></w:document>"));
    }

    #[test]
    fn namespace_helper_handles_layout_variants_and_is_idempotent() {
        // Bare root, and a root whose attribute value contains '>'.
        assert!(ensure_math_namespace("<w:document><w:body/></w:document>")
            .unwrap()
            .contains(&format!("<w:document {NS}>")));
        let tricky = ensure_math_namespace("<w:document a=\"x>y\" b='z>'>body</w:document>").unwrap();
        assert!(tricky.contains(&format!("<w:document {NS} a=\"x>y\"")) && tricky.ends_with(">body</w:document>"));
        // Already declared (either quote style): unchanged. Never duplicated.
        let once = ensure_math_namespace("<w:document x=\"1\">t</w:document>").unwrap();
        assert_eq!(ensure_math_namespace(&once).unwrap(), once);
        let single = "<w:document xmlns:m='http://schemas.openxmlformats.org/officeDocument/2006/math'>t</w:document>";
        assert_eq!(ensure_math_namespace(single).unwrap(), single);
        // A sibling element sharing the prefix of the root name is not the root.
        let out = ensure_math_namespace("<w:documentProtection/><w:document>t</w:document>").unwrap();
        assert!(out.starts_with("<w:documentProtection/>") && out.contains(&format!("<w:document {NS}>")));
    }

    #[test]
    fn namespace_helper_rejects_conflicts_and_garbage() {
        assert!(ensure_math_namespace("<w:document xmlns:m=\"http://openxmlformats.org\"></w:document>").is_err());
        assert!(ensure_math_namespace("<w:body/>").is_err());
        assert!(ensure_math_namespace("<w:document a=\"unterminated").is_err());
        // A declaration elsewhere in the document does not count for the root.
        let out = ensure_math_namespace("<w:document><x xmlns:m=\"urn:other\"/></w:document>").unwrap();
        assert!(out.contains(&format!("<w:document {NS}>")));
    }

    #[test]
    fn fraction_and_scripts() {
        let x = conv(r"\frac{a}{b} + x_i^2");
        assert!(x.contains("<m:f><m:num>"));
        assert!(x.contains("<m:sSubSup><m:e>"));
    }

    #[test]
    fn greek_and_symbols_are_unicode() {
        let x = conv(r"\alpha \leq \infty");
        assert!(x.contains('α') && x.contains('≤') && x.contains('∞'));
    }

    #[test]
    fn nary_takes_following_term_as_body() {
        let x = conv(r"\sum_{i=1}^{n} a_i + b");
        assert!(x.contains(r#"<m:chr m:val="∑"/>"#));
        let body_start = x.find("</m:sup><m:e>").unwrap();
        assert!(x[body_start..].contains("<m:sSub>"));
        // `+ b` stays outside the n-ary operand.
        assert!(x.rfind("</m:nary>").unwrap() < x.rfind("+b</m:t>").unwrap());
    }

    #[test]
    fn integral_uses_sub_sup_placement() {
        assert!(conv(r"\int_0^1 f(x)\,dx").contains(r#"<m:limLoc m:val="subSup"/>"#));
    }

    #[test]
    fn roots() {
        assert!(conv(r"\sqrt{x}").contains(r#"<m:degHide m:val="1"/>"#));
        assert!(conv(r"\sqrt[3]{x}").contains("<m:deg><m:r>"));
    }

    #[test]
    fn left_right_and_matrix() {
        let x = conv(r"\left( \begin{matrix} a & b \\ c & d \end{matrix} \right)");
        assert!(x.contains("<m:d>") && x.contains("<m:m>"));
        assert_eq!(x.matches("<m:mr>").count(), 2);
    }

    #[test]
    fn primes_become_superscripts() {
        assert!(conv("f'").contains("<m:sSup>"));
    }

    #[test]
    fn text_is_normal_text_and_escaped() {
        let x = conv(r"\text{a < b & c}");
        assert!(x.contains("<m:nor/>") && x.contains("a &lt; b &amp; c"));
    }

    #[test]
    fn unknown_command_is_kept_visible() {
        assert!(conv(r"\foobar").contains(r"\foobar"));
    }

    #[test]
    fn hostile_nesting_is_an_error_not_a_stack_overflow() {
        let n = 200_000;
        let cases = [
            format!("{}x{}", "{".repeat(n), "}".repeat(n)),
            format!("x{}1{}", "^{".repeat(n), "}".repeat(n)),
            format!("{}x{}", "\\frac{".repeat(n), "}{1}".repeat(n)),
            format!("{}x", "\\mathbf".repeat(n)),
            format!("{}x", "\\frac".repeat(n)),
            format!("{}x{}", "\\left(".repeat(n), "\\right)".repeat(n)),
            format!("{}a{}", "\\begin{matrix}".repeat(n), "\\end{matrix}".repeat(n)),
            "\\sqrt[".repeat(n),
            "\\text{".repeat(n),
        ];
        for (k, c) in cases.iter().enumerate() {
            assert!(latex_to_omml(c).is_err(), "case {k} should be rejected");
        }
        // Realistic nesting is fine.
        assert!(latex_to_omml(r"\frac{\sqrt{\frac{a}{b}}}{\left(\sum_{i}^{n} x_{i}^{2}\right)}").is_ok());
        assert!(latex_to_omml(&format!("{}x{}", "{".repeat(40), "}".repeat(40))).is_ok());
    }

    #[test]
    fn scripts_inside_sqrt_index_terminate() {
        // Found by the randomised test: `^`/`_` with an empty base inside `[..]` looped forever.
        for hang in [
            r"\sqrt[^]{x}",
            r"\sqrt[_]{x}",
            r"\sqrt[^\left |\alpha _=\frac\end{pmatrix}]\sqrt|%}1\\mathbf",
            r"\sqrt[^^^^",
        ] {
            let _ = latex_to_omml(hang); // any result, but it must return
        }
        assert!(latex_to_omml(r"\sqrt[3]{x}").is_ok());
        assert!(latex_to_omml(r"\sqrt[n^2]{x}").is_ok());
        assert!(
            latex_to_omml(r"\sqrt[{a]}b]{x}").is_ok(),
            "a `]` inside braces belongs to the group"
        );
        assert!(latex_to_omml(r"\sqrt[3").is_err());
    }

    #[test]
    fn structural_errors() {
        assert!(latex_to_omml(r"\frac{a}{b").is_err());
        assert!(latex_to_omml("a}").is_err());
        assert!(latex_to_omml(r"\left( x").is_err());
        assert!(latex_to_omml(r"\begin{matrix} a").is_err());
        assert!(latex_to_omml(r"\begin{nope} a \end{nope}").is_err());
        assert!(latex_to_omml("x^1^2").is_err());
        assert!(latex_to_omml(r"\frac{a}").is_err());
        assert!(latex_to_omml("a \\").is_err());
    }
}
