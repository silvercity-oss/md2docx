# Syntax highlighting

`md2docx` can colour the code in fenced, indented and `<pre>` code blocks.
It is off by default; switch it on with a theme:

```rust
use md2docx::{Config, Highlighting, Theme};

let cfg = Config::untrusted().with_highlighting(Highlighting::new(Theme::vscode_dark()));
```

or, with the CLI, `md2docx --highlight light|dark`.

With highlighting on, **every** code block takes the theme's background and text
colour (so a dark theme is consistent); a block whose language no highlighter
handles is drawn plain on that background. The language is the first word of the
fence's info string, lower-cased: `` ```Rust,ignore `` is `rust`.

## What is built in

Hand-written lexers, with no dependencies, for **Rust, Python, JavaScript,
TypeScript, Java, C, C++, C#, Go, Kotlin, Swift, Ruby, PHP, SQL, Bash, JSON, YAML,
TOML, HTML/XML, CSS and diffs** (and common aliases: `rs`, `py`, `js`, `ts`, `sh`,
`yml`, `c++` ...). They colour keywords, control flow, strings, comments, numbers,
function and type names, constants, variables, object keys, tags and attributes.

They are *lexers, not parsers*. They read the text once, left to right, never
backtrack and never recurse, so they take linear time on any input: that is why they
are safe for untrusted documents. The price is accuracy. Expect these gaps:

* no semantic knowledge: a name is a function only if a `(` follows it or a keyword
  such as `fn`/`def` precedes it; types are recognised from a list and, in some
  languages, from capitalisation;
* nested template strings, heredocs, string interpolation (`"$HOME"` is one string),
  regex literals, Rust lifetimes and macros that define syntax are not understood;
* JavaScript/TypeScript and C++ are coloured as one dialect each.

Themes: `Theme::vscode_light()` and `Theme::vscode_dark()` use the token colours of
Visual Studio Code's *Light+* and *Dark+* (values from microsoft/vscode, MIT). Build
your own with `Theme::new(name, background, foreground).style(TokenClass::Keyword,
TokenStyle::color("FF0000").bold())`; every colour is six hex digits and is validated
before any work is done.

## Plugging in a better highlighter

Anything that can turn `(language, source)` into byte ranges with a class can colour
a block. Implement the `Highlighter` trait (a closure also works), return `Ok(None)`
for languages you do not handle, and register it:

```rust
Highlighting::new(theme)
    .with_highlighter(my_highlighter)            // every language without a specific entry
    .with_language("rust", my_rust_highlighter)  // one language; wins over the above
```

The built-in lexers are the default. A highlighter of yours that returns `Ok(None)`
leaves that block *plain*; to fall back to the built-in lexers instead, call
`BuiltinHighlighter.highlight(lang, source)` yourself.

The contract, checked on every call: spans sorted by `start`, not overlapping,
non-empty, inside the source and on character boundaries. Anything else is an error
(`CrateError::ExternalToolFailed`) rather than a guess. A panic is caught and
reported the same way. Sources longer than `Limits::max_highlight_bytes` (256 KiB;
64 KiB in `Config::untrusted()`) are never passed to a highlighter. Highlighters
cannot be interrupted, so one that can be slow must bound itself.

The two examples below were compiled and run against this crate; they are a starting
point. Their scope-to-class tables are the part you will want to tune.

### Option 1: `syntect` (TextMate grammars, many languages)

```toml
syntect = { version = "5", default-features = false, features = ["default-syntaxes", "regex-fancy", "parsing"] }
```

```rust
use md2docx::{FenceError, Highlighter, Span, TokenClass};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};
use syntect::util::LinesWithEndings;

struct Syntect {
    syntaxes: SyntaxSet,
}

impl Syntect {
    fn new() -> Self {
        Syntect { syntaxes: SyntaxSet::load_defaults_newlines() }
    }
}

/// Map the innermost scope we recognise to a token class.
fn class_of(stack: &ScopeStack) -> Option<TokenClass> {
    for scope in stack.as_slice().iter().rev() {
        let name = scope.build_string();
        let class = match name.as_str() {
            n if n.starts_with("comment") => TokenClass::Comment,
            n if n.starts_with("string") => TokenClass::String,
            n if n.starts_with("constant.numeric") => TokenClass::Number,
            n if n.starts_with("constant") => TokenClass::Constant,
            n if n.starts_with("keyword.control") => TokenClass::ControlKeyword,
            // Operators stay in the plain foreground colour.
            n if n.starts_with("keyword.operator") => continue,
            n if n.starts_with("keyword") || n.starts_with("storage.modifier") => TokenClass::Keyword,
            // `fn`, `struct`, `class` ... are scoped `storage.type.*`, but are keywords, not types.
            n if n.starts_with("storage.type.function")
                || n.starts_with("storage.type.struct")
                || n.starts_with("storage.type.class")
                || n.starts_with("storage.type.enum")
                || n.starts_with("storage.type.trait") =>
            {
                TokenClass::Keyword
            }
            n if n.starts_with("entity.name.function") || n.starts_with("support.function") => TokenClass::Function,
            n if n.starts_with("entity.name") || n.starts_with("storage.type") || n.starts_with("support.type") => TokenClass::Type,
            n if n.starts_with("variable") => TokenClass::Variable,
            _ => continue,
        };
        return Some(class);
    }
    None
}

/// Append `start..end`, merging with the previous span when the class matches.
fn push(spans: &mut Vec<Span>, start: usize, end: usize, class: Option<TokenClass>) {
    let Some(class) = class else { return };
    if start >= end {
        return;
    }
    match spans.last_mut() {
        Some(last) if last.end == start && last.class == class => last.end = end,
        _ => spans.push(Span::new(start, end, class)),
    }
}

impl Highlighter for Syntect {
    fn highlight(&self, lang: &str, source: &str) -> Result<Option<Vec<Span>>, FenceError> {
        let Some(syntax) = self.syntaxes.find_syntax_by_token(lang) else {
            return Ok(None); // not a language syntect knows: draw it plain
        };
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut spans = Vec::new();
        let mut offset = 0;
        for line in LinesWithEndings::from(source) {
            let ops = state.parse_line(line, &self.syntaxes)?;
            let (mut at, mut class) = (0, class_of(&stack));
            for (next, op) in ops {
                push(&mut spans, offset + at, offset + next, class);
                stack.apply(&op)?;
                (at, class) = (next, class_of(&stack));
            }
            push(&mut spans, offset + at, offset + line.len(), class);
            offset += line.len();
        }
        Ok(Some(spans))
    }
}
```

Use it with `Highlighting::new(theme).with_highlighter(Syntect::new())`, or register it
for chosen languages with `.with_language("kotlin", Syntect::new())`.

Caveats: `syntect` grammars are regular expressions; with the pure-Rust `regex-fancy`
backend a crafted input can make some grammars very slow. Pair it with a small
`Limits::max_highlight_bytes`, or use it only for documents you trust. The default
syntax set is also large (several MB in the binary). `syntect` does not tell you where
a `let` or `=` belongs in your theme: the example's `class_of` is where you decide.

### Option 2: `tree-sitter` (real parsers, best accuracy, per-language crates)

```toml
tree-sitter-highlight = "0.25"
tree-sitter-rust = "0.23"      # one crate per language
```

```rust
use md2docx::{FenceError, Highlighter, Span, TokenClass};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter as TsHighlighter};

/// The capture names we ask tree-sitter to report, and the class each one becomes.
const CAPTURES: &[(&str, TokenClass)] = &[
    ("keyword", TokenClass::Keyword),
    ("string", TokenClass::String),
    ("comment", TokenClass::Comment),
    ("number", TokenClass::Number),
    ("constant", TokenClass::Constant),
    ("function", TokenClass::Function),
    ("type", TokenClass::Type),
    ("variable", TokenClass::Variable),
    ("property", TokenClass::Property),
    ("attribute", TokenClass::Attribute),
];

struct TreeSitterRust {
    config: HighlightConfiguration,
}

impl TreeSitterRust {
    fn new() -> Result<Self, FenceError> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )?;
        config.configure(&CAPTURES.iter().map(|(name, _)| *name).collect::<Vec<_>>());
        Ok(TreeSitterRust { config })
    }
}

impl Highlighter for TreeSitterRust {
    fn highlight(&self, _lang: &str, source: &str) -> Result<Option<Vec<Span>>, FenceError> {
        let mut highlighter = TsHighlighter::new();
        let events = highlighter.highlight(&self.config, source.as_bytes(), None, |_| None)?;
        let (mut open, mut spans) = (Vec::new(), Vec::new());
        for event in events {
            match event? {
                HighlightEvent::HighlightStart(h) => open.push(CAPTURES[h.0].1),
                HighlightEvent::HighlightEnd => {
                    open.pop();
                }
                HighlightEvent::Source { start, end } => push(&mut spans, start, end, open.last().copied()),
            }
        }
        Ok(Some(spans))
    }
}
```

Register it for the languages you want, and keep the built-in lexers for the rest:

```rust
let h = Highlighting::new(Theme::vscode_dark()).with_language("rust", TreeSitterRust::new()?);
```

(The `push` helper is the one from the `syntect` example.) Caveats: tree-sitter
grammars are C code compiled into your binary, which is a larger attack surface for
untrusted input than the built-in lexers; `tree-sitter-highlight` accepts a
cancellation flag you can trip from another thread to bound the running time. Each
language is another dependency and another capture-name table.

### Other options

* **Your own lexer** for an in-house language: implement `Highlighter` directly (the
  `Todo` example in the rustdoc of `Highlighter` is about ten lines).
* **A subprocess** (`bat`, `pygmentize`): possible, but you are then running a program on
  document text; apply the same care as for diagram tools (no shell, scrubbed
  environment, timeouts), or do not do it for untrusted input.
