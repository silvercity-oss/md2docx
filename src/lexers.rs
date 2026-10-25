//! The built-in lexers behind [`BuiltinHighlighter`](crate::BuiltinHighlighter).
//!
//! One table-driven scanner covers the C-like, scripting, SQL, shell, JSON, YAML
//! and TOML families; markup, CSS and diffs have small dedicated scanners. All of
//! them read the text once, left to right, never backtrack and never recurse, so
//! their running time is linear in the input whatever it contains. They return
//! spans that are sorted, non-overlapping and on character boundaries by
//! construction (checked by the tests with random input).

use crate::highlight::{Span, TokenClass as C};

/// The canonical name of a supported language, resolving aliases.
pub(crate) fn canonical(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "rust" | "rs" => "rust",
        "python" | "py" | "python3" => "python",
        "javascript" | "js" | "jsx" | "mjs" | "cjs" | "node" => "javascript",
        "typescript" | "ts" | "tsx" => "typescript",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "c++" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "csharp" | "cs" | "c#" => "csharp",
        "go" | "golang" => "go",
        "kotlin" | "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "ruby" | "rb" => "ruby",
        "php" => "php",
        "sql" | "postgresql" | "postgres" | "mysql" | "sqlite" | "psql" => "sql",
        "bash" | "sh" | "shell" | "zsh" | "console" | "shellscript" => "bash",
        "json" | "jsonc" | "json5" => "json",
        "yaml" | "yml" => "yaml",
        "toml" | "ini" => "toml",
        "html" | "htm" | "xml" | "svg" | "xhtml" | "xsl" | "plist" => "markup",
        "css" => "css",
        "diff" | "patch" => "diff",
        _ => return None,
    })
}

/// Spans for `src` in `lang`, or `None` when there is no lexer for it.
pub(crate) fn highlight(lang: &str, src: &str) -> Option<Vec<Span>> {
    Some(match canonical(lang)? {
        "markup" => markup(src),
        "css" => css(src),
        "diff" => diff(src),
        name => generic(grammar(name)?, src),
    })
}

// ------------------------------------------------------------ the generic scanner

struct Str {
    open: &'static str,
    close: &'static str,
    /// A backslash escapes the next character.
    escape: bool,
    /// The string may continue past a newline (otherwise it ends at the line end).
    multiline: bool,
    /// Two closing delimiters in a row are a literal one (SQL `''`).
    doubled: bool,
}

const fn s(open: &'static str, close: &'static str, escape: bool, multiline: bool) -> Str {
    Str {
        open,
        close,
        escape,
        multiline,
        doubled: false,
    }
}

struct Grammar {
    line_comments: &'static [&'static str],
    block_comments: &'static [(&'static str, &'static str)],
    strings: &'static [Str],
    keywords: &'static [&'static str],
    control: &'static [&'static str],
    types: &'static [&'static str],
    constants: &'static [&'static str],
    /// Keywords after which the next identifier is a definition of this class.
    defs: &'static [(&'static str, C)],
    ignore_case: bool,
    /// Characters besides letters, digits and `_` that may appear in identifiers.
    ident_extra: &'static str,
    /// Characters that start a variable when followed by a name (`$` in shell and PHP).
    sigils: &'static str,
    /// `#` begins a comment only at the start of a word (shell: `$#` is not one).
    hash_at_word_start: bool,
    /// Capitalised names are types (Java, C#, Kotlin, Swift, Go exported names...).
    capitalized_types: bool,
    /// A name or string followed by this character is a property (JSON/YAML/TOML keys).
    key_follow: Option<char>,
    /// `[...]` alone on a line is a section header (TOML, INI).
    section_headers: bool,
}

const NONE: Grammar = Grammar {
    line_comments: &[],
    block_comments: &[],
    strings: &[],
    keywords: &[],
    control: &[],
    types: &[],
    constants: &[],
    defs: &[],
    ignore_case: false,
    ident_extra: "",
    sigils: "",
    hash_at_word_start: false,
    capitalized_types: false,
    key_follow: None,
    section_headers: false,
};

const DQ: Str = s("\"", "\"", true, false);
const SQ: Str = s("'", "'", true, false);
const BT: Str = s("`", "`", true, true);

const C_CONTROL: &[&str] = &[
    "if", "else", "for", "while", "do", "switch", "case", "default", "break", "continue", "return", "goto", "try",
    "catch", "finally", "throw", "throws", "yield", "await", "async", "match", "when", "in", "of", "defer", "go",
    "select", "guard",
];

fn grammar(name: &str) -> Option<&'static Grammar> {
    static RUST: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[
            s("r#\"", "\"#", false, true),
            s("r\"", "\"", false, true),
            DQ,
            s("b'", "'", true, false),
        ],
        keywords: &[
            "as",
            "const",
            "crate",
            "dyn",
            "enum",
            "extern",
            "fn",
            "impl",
            "let",
            "mod",
            "move",
            "mut",
            "pub",
            "ref",
            "self",
            "Self",
            "static",
            "struct",
            "super",
            "trait",
            "type",
            "unsafe",
            "use",
            "where",
            "macro_rules",
        ],
        control: &[
            "if", "else", "for", "while", "loop", "match", "break", "continue", "return", "async", "await", "in",
        ],
        types: &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64",
            "bool", "char", "str", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc",
        ],
        constants: &["true", "false", "None", "Some", "Ok", "Err"],
        defs: &[
            ("fn", C::Function),
            ("struct", C::Type),
            ("enum", C::Type),
            ("trait", C::Type),
            ("type", C::Type),
            ("mod", C::Type),
        ],
        capitalized_types: true,
        ..NONE
    };
    static PYTHON: Grammar = Grammar {
        line_comments: &["#"],
        strings: &[s("\"\"\"", "\"\"\"", true, true), s("'''", "'''", true, true), DQ, SQ],
        keywords: &[
            "and", "as", "assert", "class", "def", "del", "from", "global", "import", "is", "lambda", "nonlocal",
            "not", "or", "pass", "with", "print", "self", "cls",
        ],
        control: &[
            "if", "elif", "else", "for", "while", "break", "continue", "return", "try", "except", "finally", "raise",
            "yield", "await", "async", "match", "case", "in",
        ],
        types: &[
            "int", "str", "float", "bool", "list", "dict", "set", "tuple", "bytes", "object", "type",
        ],
        constants: &["True", "False", "None"],
        defs: &[("def", C::Function), ("class", C::Type)],
        ..NONE
    };
    static JS: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[DQ, SQ, BT],
        keywords: &[
            "class",
            "const",
            "let",
            "var",
            "function",
            "new",
            "delete",
            "typeof",
            "instanceof",
            "void",
            "this",
            "super",
            "import",
            "export",
            "from",
            "extends",
            "static",
            "get",
            "set",
            "with",
            "debugger",
            "of",
        ],
        control: C_CONTROL,
        types: &[
            "Array", "Object", "String", "Number", "Boolean", "Promise", "Map", "Set", "Date", "Error", "Symbol",
        ],
        constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
        defs: &[("function", C::Function), ("class", C::Type)],
        ident_extra: "$",
        ..NONE
    };
    static TS: Grammar = Grammar {
        keywords: &[
            "class",
            "const",
            "let",
            "var",
            "function",
            "new",
            "delete",
            "typeof",
            "instanceof",
            "void",
            "this",
            "super",
            "import",
            "export",
            "from",
            "extends",
            "implements",
            "static",
            "get",
            "set",
            "interface",
            "type",
            "enum",
            "namespace",
            "declare",
            "abstract",
            "readonly",
            "public",
            "private",
            "protected",
            "as",
            "is",
            "keyof",
            "of",
        ],
        types: &[
            "string", "number", "boolean", "any", "unknown", "never", "object", "symbol", "bigint", "Array", "Promise",
            "Record", "Partial", "Map", "Set", "Date", "Error",
        ],
        defs: &[
            ("function", C::Function),
            ("class", C::Type),
            ("interface", C::Type),
            ("type", C::Type),
            ("enum", C::Type),
        ],
        ..JS
    };
    static JAVA: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[s("\"\"\"", "\"\"\"", true, true), DQ, SQ],
        keywords: &[
            "abstract",
            "class",
            "enum",
            "extends",
            "final",
            "implements",
            "import",
            "instanceof",
            "interface",
            "native",
            "new",
            "package",
            "private",
            "protected",
            "public",
            "static",
            "strictfp",
            "super",
            "synchronized",
            "this",
            "transient",
            "volatile",
            "var",
            "record",
            "sealed",
            "permits",
            "void",
        ],
        control: C_CONTROL,
        types: &[
            "int", "long", "short", "byte", "float", "double", "boolean", "char", "String", "Object", "List", "Map",
        ],
        constants: &["true", "false", "null"],
        defs: &[
            ("class", C::Type),
            ("interface", C::Type),
            ("enum", C::Type),
            ("record", C::Type),
        ],
        capitalized_types: true,
        ..NONE
    };
    static C_LANG: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[DQ, SQ],
        keywords: &[
            "auto", "const", "enum", "extern", "inline", "register", "restrict", "sizeof", "static", "struct",
            "typedef", "union", "volatile", "#include", "#define", "#ifdef", "#ifndef", "#endif", "#if", "#else",
            "#pragma",
        ],
        control: C_CONTROL,
        types: &[
            "int", "long", "short", "char", "float", "double", "void", "unsigned", "signed", "size_t", "bool",
            "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "FILE",
        ],
        constants: &["true", "false", "NULL", "nullptr"],
        defs: &[("struct", C::Type), ("enum", C::Type), ("union", C::Type)],
        ..NONE
    };
    static CPP: Grammar = Grammar {
        keywords: &[
            "auto",
            "class",
            "const",
            "constexpr",
            "enum",
            "explicit",
            "extern",
            "friend",
            "inline",
            "namespace",
            "new",
            "delete",
            "operator",
            "private",
            "protected",
            "public",
            "static",
            "struct",
            "template",
            "typedef",
            "typename",
            "union",
            "using",
            "virtual",
            "volatile",
            "this",
            "override",
            "final",
            "noexcept",
            "sizeof",
            "#include",
            "#define",
            "#ifdef",
            "#ifndef",
            "#endif",
            "#if",
            "#else",
            "#pragma",
        ],
        types: &[
            "int",
            "long",
            "short",
            "char",
            "float",
            "double",
            "void",
            "unsigned",
            "signed",
            "size_t",
            "bool",
            "string",
            "vector",
            "map",
            "set",
            "unique_ptr",
            "shared_ptr",
            "uint8_t",
            "uint16_t",
            "uint32_t",
            "uint64_t",
        ],
        defs: &[
            ("class", C::Type),
            ("struct", C::Type),
            ("enum", C::Type),
            ("namespace", C::Type),
        ],
        strings: &[s("R\"(", ")\"", false, true), DQ, SQ],
        ..C_LANG
    };
    static CSHARP: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[s("@\"", "\"", false, true), s("\"\"\"", "\"\"\"", false, true), DQ, SQ],
        keywords: &[
            "abstract",
            "as",
            "base",
            "class",
            "const",
            "delegate",
            "enum",
            "event",
            "explicit",
            "extern",
            "interface",
            "internal",
            "is",
            "namespace",
            "new",
            "operator",
            "out",
            "override",
            "params",
            "private",
            "protected",
            "public",
            "readonly",
            "ref",
            "sealed",
            "static",
            "struct",
            "this",
            "using",
            "virtual",
            "var",
            "record",
            "get",
            "set",
            "init",
            "void",
        ],
        control: C_CONTROL,
        types: &[
            "int",
            "long",
            "short",
            "byte",
            "float",
            "double",
            "decimal",
            "bool",
            "char",
            "string",
            "object",
            "Task",
            "List",
            "Dictionary",
        ],
        constants: &["true", "false", "null"],
        defs: &[
            ("class", C::Type),
            ("interface", C::Type),
            ("struct", C::Type),
            ("enum", C::Type),
            ("record", C::Type),
        ],
        capitalized_types: true,
        ..NONE
    };
    static GO: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[DQ, s("'", "'", true, false), s("`", "`", false, true)],
        keywords: &[
            "chan",
            "const",
            "func",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "struct",
            "type",
            "var",
            "fallthrough",
        ],
        control: &[
            "if", "else", "for", "switch", "case", "default", "break", "continue", "return", "goto", "defer", "go",
            "select",
        ],
        types: &[
            "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64", "float32",
            "float64", "string", "bool", "byte", "rune", "error", "any", "uintptr",
        ],
        constants: &["true", "false", "nil", "iota"],
        defs: &[("func", C::Function), ("type", C::Type)],
        ..NONE
    };
    static KOTLIN: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[s("\"\"\"", "\"\"\"", false, true), DQ, SQ],
        keywords: &[
            "abstract",
            "as",
            "class",
            "companion",
            "const",
            "data",
            "enum",
            "fun",
            "import",
            "inline",
            "interface",
            "internal",
            "is",
            "lateinit",
            "object",
            "open",
            "override",
            "package",
            "private",
            "protected",
            "public",
            "sealed",
            "super",
            "this",
            "typealias",
            "val",
            "var",
            "by",
            "init",
            "suspend",
        ],
        control: C_CONTROL,
        types: &[
            "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "String", "Any", "Unit", "List",
            "Map",
        ],
        constants: &["true", "false", "null"],
        defs: &[
            ("fun", C::Function),
            ("class", C::Type),
            ("interface", C::Type),
            ("object", C::Type),
        ],
        capitalized_types: true,
        ..NONE
    };
    static SWIFT: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[s("\"\"\"", "\"\"\"", true, true), DQ],
        keywords: &[
            "associatedtype",
            "class",
            "deinit",
            "enum",
            "extension",
            "fileprivate",
            "func",
            "import",
            "init",
            "inout",
            "internal",
            "let",
            "open",
            "operator",
            "private",
            "protocol",
            "public",
            "static",
            "struct",
            "subscript",
            "typealias",
            "var",
            "self",
            "Self",
            "super",
            "is",
            "as",
            "lazy",
            "weak",
            "mutating",
            "override",
        ],
        control: C_CONTROL,
        types: &[
            "Int",
            "Double",
            "Float",
            "Bool",
            "String",
            "Character",
            "Array",
            "Dictionary",
            "Set",
            "Optional",
            "Any",
        ],
        constants: &["true", "false", "nil"],
        defs: &[
            ("func", C::Function),
            ("class", C::Type),
            ("struct", C::Type),
            ("enum", C::Type),
            ("protocol", C::Type),
        ],
        capitalized_types: true,
        ..NONE
    };
    static RUBY: Grammar = Grammar {
        line_comments: &["#"],
        strings: &[DQ, SQ, BT],
        keywords: &[
            "alias",
            "class",
            "def",
            "defined?",
            "end",
            "module",
            "nil",
            "not",
            "or",
            "and",
            "self",
            "super",
            "undef",
            "begin",
            "ensure",
            "rescue",
            "require",
            "require_relative",
            "include",
            "extend",
            "attr_accessor",
            "attr_reader",
            "attr_writer",
            "puts",
            "lambda",
            "proc",
        ],
        control: &[
            "if", "elsif", "else", "unless", "case", "when", "while", "until", "for", "do", "break", "next", "return",
            "yield", "then", "in", "redo", "retry",
        ],
        types: &[],
        constants: &["true", "false", "nil"],
        defs: &[("def", C::Function), ("class", C::Type), ("module", C::Type)],
        ident_extra: "?!",
        capitalized_types: true,
        ..NONE
    };
    static PHP: Grammar = Grammar {
        line_comments: &["//", "#"],
        block_comments: &[("/*", "*/")],
        strings: &[DQ, SQ],
        keywords: &[
            "abstract",
            "class",
            "const",
            "echo",
            "extends",
            "final",
            "function",
            "global",
            "implements",
            "include",
            "include_once",
            "interface",
            "namespace",
            "new",
            "print",
            "private",
            "protected",
            "public",
            "require",
            "require_once",
            "static",
            "trait",
            "use",
            "var",
            "fn",
            "readonly",
            "enum",
            "<?php",
            "?>",
        ],
        control: &[
            "if", "else", "elseif", "for", "foreach", "while", "do", "switch", "case", "default", "break", "continue",
            "return", "try", "catch", "finally", "throw", "match", "yield", "as",
        ],
        types: &[
            "int", "float", "string", "bool", "array", "object", "mixed", "void", "callable", "iterable",
        ],
        constants: &["true", "false", "null", "TRUE", "FALSE", "NULL"],
        defs: &[
            ("function", C::Function),
            ("class", C::Type),
            ("interface", C::Type),
            ("trait", C::Type),
        ],
        sigils: "$",
        capitalized_types: true,
        ..NONE
    };
    static SQL: Grammar = Grammar {
        line_comments: &["--"],
        block_comments: &[("/*", "*/")],
        strings: &[Str {
            doubled: true,
            ..s("'", "'", false, true)
        }],
        keywords: &[
            "select",
            "from",
            "where",
            "insert",
            "into",
            "values",
            "update",
            "set",
            "delete",
            "create",
            "table",
            "alter",
            "drop",
            "index",
            "view",
            "join",
            "inner",
            "left",
            "right",
            "full",
            "outer",
            "cross",
            "on",
            "as",
            "group",
            "by",
            "order",
            "having",
            "limit",
            "offset",
            "union",
            "all",
            "distinct",
            "and",
            "or",
            "not",
            "in",
            "is",
            "like",
            "between",
            "exists",
            "primary",
            "key",
            "foreign",
            "references",
            "default",
            "unique",
            "check",
            "constraint",
            "begin",
            "commit",
            "rollback",
            "transaction",
            "with",
            "returning",
            "asc",
            "desc",
            "case",
            "when",
            "then",
            "else",
            "end",
            "if",
            "using",
            "truncate",
            "grant",
            "revoke",
            "database",
            "schema",
            "explain",
            "analyze",
            "replace",
            "over",
            "partition",
            "window",
            "recursive",
            "cascade",
        ],
        control: &[],
        types: &[
            "int",
            "integer",
            "bigint",
            "smallint",
            "serial",
            "bigserial",
            "text",
            "varchar",
            "char",
            "boolean",
            "bool",
            "date",
            "time",
            "timestamp",
            "timestamptz",
            "numeric",
            "decimal",
            "real",
            "float",
            "double",
            "json",
            "jsonb",
            "uuid",
            "blob",
            "bytea",
        ],
        constants: &["null", "true", "false"],
        defs: &[],
        ignore_case: true,
        ..NONE
    };
    static BASH: Grammar = Grammar {
        line_comments: &["#"],
        strings: &[DQ, s("'", "'", false, true), BT],
        keywords: &[
            "function", "local", "export", "readonly", "declare", "unset", "alias", "source", "cd", "echo", "printf",
            "exit", "set", "shift", "trap", "exec", "eval", "test", "read", "sudo",
        ],
        control: &[
            "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "return",
            "break", "continue", "select",
        ],
        types: &[],
        constants: &["true", "false"],
        defs: &[("function", C::Function)],
        ident_extra: "-",
        sigils: "$",
        hash_at_word_start: true,
        ..NONE
    };
    static JSON: Grammar = Grammar {
        line_comments: &["//"],
        block_comments: &[("/*", "*/")],
        strings: &[DQ],
        constants: &["true", "false", "null"],
        key_follow: Some(':'),
        ..NONE
    };
    static YAML: Grammar = Grammar {
        line_comments: &["#"],
        strings: &[DQ, s("'", "'", false, false)],
        constants: &[
            "true", "false", "null", "yes", "no", "on", "off", "True", "False", "Null", "TRUE", "FALSE", "NULL",
        ],
        ident_extra: "-.",
        sigils: "&*!",
        hash_at_word_start: true,
        key_follow: Some(':'),
        ..NONE
    };
    static TOML: Grammar = Grammar {
        line_comments: &["#", ";"],
        strings: &[
            s("\"\"\"", "\"\"\"", true, true),
            s("'''", "'''", false, true),
            DQ,
            s("'", "'", false, false),
        ],
        constants: &["true", "false"],
        ident_extra: "-.",
        hash_at_word_start: true,
        key_follow: Some('='),
        section_headers: true,
        ..NONE
    };
    Some(match name {
        "rust" => &RUST,
        "python" => &PYTHON,
        "javascript" => &JS,
        "typescript" => &TS,
        "java" => &JAVA,
        "c" => &C_LANG,
        "cpp" => &CPP,
        "csharp" => &CSHARP,
        "go" => &GO,
        "kotlin" => &KOTLIN,
        "swift" => &SWIFT,
        "ruby" => &RUBY,
        "php" => &PHP,
        "sql" => &SQL,
        "bash" => &BASH,
        "json" => &JSON,
        "yaml" => &YAML,
        "toml" => &TOML,
        _ => return None,
    })
}

fn in_list(list: &[&str], word: &str, ignore_case: bool) -> bool {
    list.iter().any(|w| {
        if ignore_case {
            w.eq_ignore_ascii_case(word)
        } else {
            *w == word
        }
    })
}

fn is_ident_start(c: char, g: &Grammar) -> bool {
    c.is_alphabetic() || c == '_' || (g.ident_extra.contains(c) && !matches!(c, '-' | '.' | '?' | '!'))
}

fn is_ident_continue(c: char, g: &Grammar) -> bool {
    c.is_alphanumeric() || c == '_' || g.ident_extra.contains(c)
}

/// End of the string that opens at `i` (which starts with `rule.open`).
fn scan_string(src: &str, i: usize, rule: &Str) -> usize {
    let mut j = i + rule.open.len();
    while j < src.len() {
        let rest = &src[j..];
        if let Some(after) = rest.strip_prefix(rule.close) {
            if rule.doubled && after.starts_with(rule.close) {
                j += 2 * rule.close.len();
                continue;
            }
            return j + rule.close.len();
        }
        let Some(ch) = rest.chars().next() else { break };
        if ch == '\n' && !rule.multiline {
            return j; // unterminated: the string ends with its line
        }
        j += ch.len_utf8();
        if rule.escape && ch == '\\' {
            if let Some(next) = src[j..].chars().next() {
                if next != '\n' || rule.multiline {
                    j += next.len_utf8();
                }
            }
        }
    }
    src.len()
}

/// The first non-blank character after `at` on the same line.
fn next_on_line(src: &str, at: usize) -> Option<char> {
    src[at..].chars().find(|c| *c != ' ' && *c != '\t')
}

fn generic(g: &Grammar, src: &str) -> Vec<Span> {
    let b = src.as_bytes();
    let mut out: Vec<Span> = Vec::new();
    let mut i = 0;
    let mut pending: Option<C> = None;
    let mut line_start = true;
    'scan: while i < src.len() {
        let rest = &src[i..];
        let Some(c) = rest.chars().next() else { break };
        if c == '\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let at_line_start = line_start;
        line_start = false;
        let word_start = i == 0
            || matches!(
                b[i - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b';' | b'|' | b'&' | b'(' | b'{'
            );

        // [section] headers
        if g.section_headers && at_line_start && c == '[' {
            let end = rest.find('\n').map_or(src.len(), |n| i + n);
            let line = src[i..end].trim_end();
            if line.contains(']') {
                out.push(Span::new(i, i + line.len(), C::Type));
                i += line.len();
                continue;
            }
        }
        // comments
        for open in g.line_comments {
            if rest.starts_with(open) && (!g.hash_at_word_start || *open != "#" || word_start) {
                let end = rest.find('\n').map_or(src.len(), |n| i + n);
                out.push(Span::new(i, end, C::Comment));
                i = end;
                pending = None;
                continue 'scan;
            }
        }
        for (open, close) in g.block_comments {
            if let Some(after) = rest.strip_prefix(open) {
                let end = after
                    .find(close)
                    .map_or(src.len(), |n| i + open.len() + n + close.len());
                out.push(Span::new(i, end, C::Comment));
                i = end;
                pending = None;
                continue 'scan;
            }
        }
        // strings
        for rule in g.strings {
            if rest.starts_with(rule.open) {
                let end = scan_string(src, i, rule);
                let class = match g.key_follow {
                    Some(k) if next_on_line(src, end) == Some(k) => C::Property,
                    _ => C::String,
                };
                out.push(Span::new(i, end, class));
                i = end;
                pending = None;
                continue 'scan;
            }
        }
        // numbers
        let prev_alnum = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        let starts_number =
            c.is_ascii_digit() || (c == '.' && rest[1..].starts_with(|d: char| d.is_ascii_digit()) && !prev_alnum);
        if starts_number && !prev_alnum {
            let mut j = i;
            while j < src.len() {
                let ch = src[j..].chars().next().unwrap_or(' ');
                let more = ch.is_ascii_alphanumeric()
                    || ch == '_'
                    || (ch == '.' && src[j + 1..].starts_with(|d: char| d.is_ascii_digit()))
                    || (matches!(ch, '+' | '-')
                        && j > i
                        && matches!(b[j - 1], b'e' | b'E')
                        && src[i..j]
                            .bytes()
                            .all(|x| x.is_ascii_digit() || x == b'.' || x == b'_' || x == b'e' || x == b'E'));
                if !more {
                    break;
                }
                j += ch.len_utf8();
            }
            out.push(Span::new(i, j, C::Number));
            i = j;
            pending = None;
            continue;
        }
        // $variables, &anchors ...
        if g.sigils.contains(c) {
            let mut j = i + c.len_utf8();
            if src[j..].starts_with('{') {
                j = src[j..].find('}').map_or(src.len(), |n| j + n + 1);
            } else if src[j..].starts_with('(') {
                // `$(command)`: leave the sigil plain, the command is scanned normally
                i += c.len_utf8();
                continue;
            } else {
                let name_len: usize = src[j..]
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .map(char::len_utf8)
                    .sum();
                if name_len == 0 {
                    // `$?`, `$#`, `$@`, `$$`, `$1`... (shell special parameters)
                    match src[j..].chars().next() {
                        Some(ch) if c == '$' && "?#@*!$-".contains(ch) => j += ch.len_utf8(),
                        _ => {
                            i += c.len_utf8();
                            continue;
                        }
                    }
                } else {
                    j += name_len;
                }
            }
            out.push(Span::new(i, j, C::Variable));
            i = j;
            pending = None;
            continue;
        }
        // directives such as `#include` count as keywords
        if c == '#' && at_line_start && g.keywords.iter().any(|k| k.starts_with('#')) {
            // At most a directive's length is read: `#` followed by a long run must stay cheap.
            let word: String = std::iter::once('#')
                .chain(rest[1..].chars().take_while(|ch| ch.is_ascii_alphabetic()).take(12))
                .collect();
            if in_list(g.keywords, &word, false) {
                out.push(Span::new(i, i + word.len(), C::Keyword));
                i += word.len();
                continue;
            }
        }
        if g.keywords.contains(&"<?php") && (rest.starts_with("<?php") || rest.starts_with("?>")) {
            let n = if rest.starts_with("<?php") { 5 } else { 2 };
            out.push(Span::new(i, i + n, C::Keyword));
            i += n;
            continue;
        }
        // identifiers
        if is_ident_start(c, g) {
            let mut j = i;
            for ch in rest.chars() {
                if is_ident_continue(ch, g) {
                    j += ch.len_utf8();
                } else {
                    break;
                }
            }
            // Ruby's `?`/`!` only belong at the end of a name; do not swallow `a ? b : c` etc.
            let word = &src[i..j];
            let next = next_on_line(src, j);
            let class = if let Some(def) = pending.take() {
                Some(def)
            } else if g
                .key_follow
                .is_some_and(|k| next == Some(k) && key_follow_ok(src, j, k))
            {
                Some(C::Property)
            } else if in_list(g.keywords, word, g.ignore_case) {
                if let Some((_, def)) = g.defs.iter().find(|(k, _)| *k == word) {
                    pending = Some(*def);
                }
                Some(C::Keyword)
            } else if in_list(g.control, word, g.ignore_case) {
                Some(C::ControlKeyword)
            } else if in_list(g.constants, word, g.ignore_case) {
                Some(C::Constant)
            } else if in_list(g.types, word, g.ignore_case) {
                Some(C::Type)
            } else if next == Some('(') && !g.ignore_case {
                Some(C::Function)
            } else if g.capitalized_types
                && word.starts_with(char::is_uppercase)
                && word.chars().any(char::is_lowercase)
            {
                Some(C::Type)
            } else {
                None
            };
            if let Some(class) = class {
                out.push(Span::new(i, j, class));
            }
            i = j;
            continue;
        }
        // anything else (operators, punctuation) is drawn plain
        pending = None;
        i += c.len_utf8();
    }
    out
}

/// A `:` ends a key only when followed by a space or the end of the line (so URLs are not
/// keys); `=` always does.
fn key_follow_ok(src: &str, after_name: usize, key: char) -> bool {
    if key != ':' {
        return true;
    }
    let rest = src[after_name..].trim_start_matches([' ', '\t']);
    rest.strip_prefix(':')
        .is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t', '\n', '\r']))
}

// ------------------------------------------------------------ markup, CSS, diff

fn markup(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let Some(open) = src[i..].find('<') else { break };
        let at = i + open;
        let rest = &src[at..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(src.len(), |n| at + n + 3);
            out.push(Span::new(at, end, C::Comment));
            i = end;
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest.find('>').map_or(src.len(), |n| at + n + 1);
            out.push(Span::new(at, end, C::Keyword));
            i = end;
            continue;
        }
        let name_at = at + 1 + usize::from(rest[1..].starts_with('/'));
        let name_len: usize = src[name_at..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
            .map(char::len_utf8)
            .sum();
        if name_len == 0 || !src[name_at..].starts_with(|c: char| c.is_alphabetic()) {
            i = at + 1; // a lone `<` is text
            continue;
        }
        out.push(Span::new(at, name_at + name_len, C::Tag));
        let mut j = name_at + name_len;
        // attributes up to the closing `>`
        while j < src.len() {
            let ch = src[j..].chars().next().unwrap_or('>');
            if ch == '>' || (ch == '/' && src[j..].starts_with("/>")) {
                break;
            }
            if ch == '"' || ch == '\'' {
                let end = src[j + 1..].find(ch).map_or(src.len(), |n| j + 1 + n + 1);
                out.push(Span::new(j, end, C::String));
                j = end;
            } else if ch.is_alphabetic() || matches!(ch, '_' | ':' | '@') {
                let len: usize = src[j..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.' | '@'))
                    .map(char::len_utf8)
                    .sum();
                out.push(Span::new(j, j + len, C::Attribute));
                j += len;
            } else {
                j += ch.len_utf8();
            }
        }
        i = j;
    }
    out
}

fn css(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    let mut in_value = false;
    while i < src.len() {
        let rest = &src[i..];
        let Some(c) = rest.chars().next() else { break };
        if let Some(after) = rest.strip_prefix("/*") {
            let end = after.find("*/").map_or(src.len(), |n| i + 2 + n + 2);
            out.push(Span::new(i, end, C::Comment));
            i = end;
        } else if c == '"' || c == '\'' {
            let end = scan_string(
                src,
                i,
                &s(
                    if c == '"' { "\"" } else { "'" },
                    if c == '"' { "\"" } else { "'" },
                    true,
                    false,
                ),
            );
            out.push(Span::new(i, end, C::String));
            i = end;
        } else if c == '{' {
            depth += 1;
            in_value = false;
            i += 1;
        } else if c == '}' {
            depth = depth.saturating_sub(1);
            in_value = false;
            i += 1;
        } else if c == ';' {
            in_value = false;
            i += 1;
        } else if c == ':' && depth > 0 {
            in_value = true;
            i += 1;
        } else if c == '@' {
            let len: usize = rest
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || matches!(ch, '@' | '-' | '_'))
                .map(char::len_utf8)
                .sum();
            out.push(Span::new(i, i + len, C::Keyword));
            i += len;
        } else if c == '#' && in_value {
            let len: usize = rest[1..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .map(char::len_utf8)
                .sum();
            if len > 0 {
                out.push(Span::new(i, i + 1 + len, C::Number));
                i += 1 + len;
            } else {
                i += 1;
            }
        } else if c.is_ascii_digit() || (c == '.' && rest[1..].starts_with(|d: char| d.is_ascii_digit())) {
            let len: usize = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '%')
                .map(char::len_utf8)
                .sum();
            out.push(Span::new(i, i + len, C::Number));
            i += len;
        } else if c.is_alphabetic() || matches!(c, '-' | '_' | '.' | '#' | '*') {
            let len: usize = rest
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | '#' | '*' | '%'))
                .map(char::len_utf8)
                .sum::<usize>()
                .max(c.len_utf8());
            let word = &rest[..len];
            let after = next_on_line(src, i + len);
            let class = if depth == 0 {
                // a selector
                if word.starts_with('.') || word.starts_with('#') {
                    Some(C::Type)
                } else {
                    Some(C::Tag)
                }
            } else if !in_value && after == Some(':') {
                Some(C::Property)
            } else if in_value && after == Some('(') {
                Some(C::Function)
            } else {
                None
            };
            if let Some(class) = class {
                out.push(Span::new(i, i + len, class));
            }
            i += len;
        } else {
            i += c.len_utf8();
        }
    }
    out
}

fn diff(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in src.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if !content.is_empty() {
            let class = if content.starts_with("+++")
                || content.starts_with("---")
                || content.starts_with("diff ")
                || content.starts_with("index ")
            {
                Some(C::Heading)
            } else if content.starts_with("@@") {
                Some(C::Function)
            } else if content.starts_with('+') {
                Some(C::Inserted)
            } else if content.starts_with('-') {
                Some(C::Deleted)
            } else {
                None
            };
            if let Some(class) = class {
                out.push(Span::new(offset, offset + content.len(), class));
            }
        }
        offset += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::validate_spans;

    /// `(text, class)` pairs of a highlighted snippet.
    fn tokens<'a>(lang: &str, src: &'a str) -> Vec<(&'a str, C)> {
        highlight(lang, src)
            .unwrap_or_else(|| panic!("no lexer for {lang}"))
            .iter()
            .map(|s| (&src[s.start..s.end], s.class))
            .collect()
    }

    fn has(lang: &str, src: &str, text: &str, class: C) {
        let t = tokens(lang, src);
        assert!(
            t.contains(&(text, class)),
            "[{lang}] `{text}` as {class:?} missing in {t:?}"
        );
    }

    #[test]
    fn rust() {
        let src = "// note\nfn main() {\n    let x: u32 = 0x1F; println!(\"hi\\n\");\n    if x > 1 { return; }\n}\nstruct Point;";
        has("rust", src, "// note", C::Comment);
        has("rust", src, "fn", C::Keyword);
        has("rust", src, "main", C::Function);
        has("rust", src, "let", C::Keyword);
        has("rust", src, "u32", C::Type);
        has("rust", src, "0x1F", C::Number);
        has("rust", src, "\"hi\\n\"", C::String);
        has("rust", src, "if", C::ControlKeyword);
        has("rust", src, "Point", C::Type);
        has("rs", "let r = r#\"a \" b\"#;", "r#\"a \" b\"#", C::String);
    }

    #[test]
    fn python() {
        let src =
            "# c\nclass A:\n    def f(self, x=1.5e-3):\n        '''doc\nmore'''\n        return None if x else True\n";
        has("python", src, "# c", C::Comment);
        has("python", src, "class", C::Keyword);
        has("py", src, "A", C::Type);
        has("py", src, "def", C::Keyword);
        has("py", src, "f", C::Function);
        has("py", src, "1.5e-3", C::Number);
        has("py", src, "'''doc\nmore'''", C::String);
        has("py", src, "return", C::ControlKeyword);
        has("py", src, "None", C::Constant);
    }

    #[test]
    fn javascript_and_typescript() {
        let src = "const s = `a ${b}`; // x\nfunction go(n) { return n >= 10 ? 'y' : \"n\"; }\nclass K extends Base {}";
        has("js", src, "const", C::Keyword);
        has("js", src, "`a ${b}`", C::String);
        has("js", src, "go", C::Function);
        has("js", src, "10", C::Number);
        has("js", src, "K", C::Type);
        has("ts", "interface Shape { n: number }", "number", C::Type);
        has("ts", "interface Shape { n: number }", "Shape", C::Type);
    }

    #[test]
    fn c_family_and_go() {
        has(
            "c",
            "#include <stdio.h>\nint main(void) { return 0; }",
            "#include",
            C::Keyword,
        );
        has(
            "c",
            "#include <stdio.h>\nint main(void) { return 0; }",
            "main",
            C::Function,
        );
        has(
            "cpp",
            "template <typename T> class Box { std::string s; };",
            "template",
            C::Keyword,
        );
        has("java", "public class Main { String s = \"x\"; }", "Main", C::Type);
        has(
            "csharp",
            "namespace N { class P { string s = @\"a\\b\"; } }",
            "@\"a\\b\"",
            C::String,
        );
        has("go", "func main() { x := 1 }\ntype T struct{}", "func", C::Keyword);
        has("go", "func main() { x := 1 }", "main", C::Function);
        has("kotlin", "fun f(): Int = 1", "fun", C::Keyword);
        has("swift", "func f() -> Int { nil }", "nil", C::Constant);
    }

    #[test]
    fn scripting_languages() {
        has("ruby", "def hi?(x)\n  puts 'a'\nend", "def", C::Keyword);
        has("ruby", "def hi?(x)\n  puts 'a'\nend", "hi?", C::Function);
        has("php", "<?php $a = 1; echo \"x\"; // c", "$a", C::Variable);
        has("php", "<?php $a = 1; echo \"x\"; // c", "<?php", C::Keyword);
        let sh = "#!/bin/sh\nset -e\nif [ -n $HOME ]; then echo ${USER} $# $(date); fi # tail\nx=$1";
        has("bash", sh, "#!/bin/sh", C::Comment);
        has("bash", sh, "if", C::ControlKeyword);
        has("bash", sh, "$HOME", C::Variable);
        has("bash", sh, "${USER}", C::Variable);
        has("bash", sh, "$#", C::Variable);
        has("bash", sh, "# tail", C::Comment);
        has("bash", sh, "$1", C::Variable);
        // `$#` is not a comment start
        assert!(!tokens("bash", "echo $# done").iter().any(|(_, c)| *c == C::Comment));
    }

    #[test]
    fn sql() {
        let src = "SELECT name, 'it''s' AS n -- c\nFROM t WHERE id >= 10 /* b */ AND x IS NULL;";
        has("sql", src, "SELECT", C::Keyword);
        has("sql", src, "'it''s'", C::String);
        has("sql", src, "-- c", C::Comment);
        has("sql", src, "10", C::Number);
        has("sql", src, "/* b */", C::Comment);
        has("sql", src, "NULL", C::Constant);
    }

    #[test]
    fn data_formats() {
        let json = "{\"a\": [1, 2.5e3, true, null], \"b\": \"x\\\"y\"}";
        has("json", json, "\"a\"", C::Property);
        has("json", json, "2.5e3", C::Number);
        has("json", json, "true", C::Constant);
        has("json", json, "\"x\\\"y\"", C::String);
        let yaml = "# c\nname: demo\nurl: http://x.example/a\nitems:\n  - &a one\n  - \"two\"\nok: true # t\nn: 3\n";
        has("yaml", yaml, "# c", C::Comment);
        has("yaml", yaml, "name", C::Property);
        has("yaml", yaml, "true", C::Constant);
        has("yaml", yaml, "\"two\"", C::String);
        has("yaml", yaml, "3", C::Number);
        has("yaml", yaml, "# t", C::Comment);
        assert!(
            !tokens("yaml", yaml).contains(&("http", C::Property)),
            "a URL is not a key"
        );
        let toml = "# c\n[package]\nname = \"x\"\nversion = '1'\nn = 0x10\nok = true\n[[bin]]\n";
        has("toml", toml, "[package]", C::Type);
        has("toml", toml, "name", C::Property);
        has("toml", toml, "\"x\"", C::String);
        has("toml", toml, "true", C::Constant);
        has("toml", toml, "[[bin]]", C::Type);
    }

    #[test]
    fn markup_css_and_diff() {
        let html = "<!DOCTYPE html>\n<!-- c --><a href=\"x\" data-n='1' hidden>t &amp; u</a><br/>";
        has("html", html, "<!DOCTYPE html>", C::Keyword);
        has("html", html, "<!-- c -->", C::Comment);
        has("html", html, "<a", C::Tag);
        has("html", html, "href", C::Attribute);
        has("html", html, "\"x\"", C::String);
        has("html", html, "</a", C::Tag);
        has(
            "xml",
            "<?xml version=\"1.0\"?><r/>",
            "<?xml version=\"1.0\"?>",
            C::Keyword,
        );
        let css =
            "/* c */\n@media (min-width: 10px) { a.b:hover, #id { color: #fff; margin: 0 2px; background: url('x') } }";
        has("css", css, "/* c */", C::Comment);
        has("css", css, "@media", C::Keyword);
        has("css", css, "color", C::Property);
        has("css", css, "#fff", C::Number);
        has("css", css, "2px", C::Number);
        has("css", css, "'x'", C::String);
        has("css", css, "url", C::Function);
        let diff = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n";
        has("diff", diff, "diff --git a/f b/f", C::Heading);
        has("patch", diff, "@@ -1,2 +1,2 @@", C::Function);
        has("diff", diff, "-old", C::Deleted);
        has("diff", diff, "+new", C::Inserted);
    }

    #[test]
    fn aliases_and_unknown_languages() {
        for (alias, canon) in [
            ("rs", "rust"),
            ("py", "python"),
            ("sh", "bash"),
            ("yml", "yaml"),
            ("c++", "cpp"),
            ("htm", "markup"),
            ("ts", "typescript"),
        ] {
            assert_eq!(canonical(alias), Some(canon), "{alias}");
        }
        assert!(highlight("brainfuck", "+++").is_none());
        assert!(highlight("", "x").is_none());
        assert!(highlight("text", "x").is_none());
    }

    const LANGS: &[&str] = &[
        "rust", "python", "js", "ts", "java", "c", "cpp", "csharp", "go", "kotlin", "swift", "ruby", "php", "sql",
        "bash", "json", "yaml", "toml", "html", "css", "diff",
    ];

    #[test]
    fn spans_are_always_valid_on_random_and_hostile_text() {
        // A small deterministic generator (no dependency) over an alphabet of syntax characters.
        let alphabet: Vec<&str> = vec![
            "\"",
            "'",
            "`",
            "/",
            "*",
            "\\",
            "#",
            "$",
            "{",
            "}",
            "(",
            ")",
            "[",
            "]",
            "<",
            ">",
            "!",
            "-",
            "=",
            ":",
            ";",
            "\n",
            "\r\n",
            " ",
            "\t",
            "a",
            "Z",
            "_",
            "0",
            "9",
            ".",
            "e",
            "é",
            "日",
            "\u{1F600}",
            "/*",
            "*/",
            "//",
            "<!--",
            "-->",
            "\"\"\"",
            "'''",
            "r#\"",
            "<?php",
            "?>",
            "@@",
            "+++",
            "fn ",
            "def ",
            "class ",
            "SELECT ",
            "if ",
            "true",
            "&",
            "%",
            "\u{0}",
            "\u{7f}",
        ];
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..300 {
            let len = (next() % 120) as usize;
            let src: String = (0..len)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect();
            for lang in LANGS {
                let spans = highlight(lang, &src).unwrap();
                if let Err(e) = validate_spans(&spans, &src) {
                    panic!("[{lang}] round {round}: {e}\nsource: {src:?}\nspans: {spans:?}");
                }
            }
        }
    }

    #[test]
    fn running_time_is_linear_on_adversarial_input() {
        let unit_sets: &[&str] = &[
            "\"",
            "'",
            "`",
            "/*",
            "//",
            "<!--",
            "<a ",
            "\"\"\"",
            "r#\"",
            "$",
            "${",
            "@",
            "#",
            "[",
            "{",
            "\\",
            "e+",
            "0x",
            ".5",
            "&",
            "<",
            "a:",
            "a=",
            "-",
            "+",
            "#if",
            "#include",
            "\n#define",
            "#ifdef#",
            "\n#",
        ];
        for unit in unit_sets {
            let src = unit.repeat(50_000 / unit.len());
            for lang in LANGS {
                let started = std::time::Instant::now();
                let spans = highlight(lang, &src).unwrap();
                validate_spans(&spans, &src).unwrap();
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(5),
                    "[{lang}] {unit:?} x{} took {:?}",
                    src.len() / unit.len(),
                    started.elapsed()
                );
            }
        }
    }
}
