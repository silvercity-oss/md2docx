//! Syntax highlighting of code blocks: themes, the built-in lexers through the whole
//! pipeline, custom highlighters, and what a hostile highlighter or document cannot do.

mod common;

use common::*;
use silvercity_oss::{
    markdown_to_docx, BuiltinHighlighter, CodeBlockLayout, Config, CrateError, DocumentOptions, FenceError,
    Highlighter, Highlighting, ImageAccess, Span, Theme, TokenClass, TokenStyle,
};

/// A highlighter as a plain function.
type Hl = fn(&str, &str) -> Result<Option<Vec<Span>>, FenceError>;

fn light() -> Config {
    Config::native_math()
        .with_image_access(ImageAccess::None)
        .with_highlighting(Highlighting::new(Theme::vscode_light()))
}

fn dark() -> Config {
    Config::native_math()
        .with_image_access(ImageAccess::None)
        .with_highlighting(Highlighting::new(Theme::vscode_dark()))
}

fn fenced(lang: &str, code: &str) -> String {
    format!("```{lang}\n{code}\n```\n")
}

fn code_paragraphs(doc: &str) -> Vec<&str> {
    doc.split("<w:p ")
        .skip(1)
        .filter(|p| p.contains("w:val=\"CodeBlock\""))
        .map(|p| p.split("</w:p>").next().unwrap())
        .collect()
}

/// The text of the code paragraphs as a reader would copy it.
fn copied_text(doc: &str) -> String {
    code_paragraphs(doc)
        .iter()
        .map(|p| {
            let mut out = String::new();
            let mut rest = *p;
            while let Some(i) = rest.find('<') {
                rest = &rest[i..];
                if rest.starts_with("<w:tab/>") {
                    out.push('\t');
                } else if rest.starts_with("<w:br") {
                    out.push('\n');
                } else if rest.starts_with("<w:t>") || rest.starts_with("<w:t ") {
                    let gt = rest.find('>').unwrap();
                    let end = rest.find("</w:t>").unwrap();
                    out.push_str(&rest[gt + 1..end]);
                    rest = &rest[end..];
                    continue;
                }
                rest = &rest[1..];
            }
            out.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The run containing `text`, as XML.
fn run_with<'a>(doc: &'a str, text: &str) -> &'a str {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    let at = doc
        .find(&format!(">{escaped}</w:t>"))
        .unwrap_or_else(|| panic!("`{text}` not in a run of its own:\n{doc}"));
    let start = doc[..at].rfind("<w:r>").unwrap();
    &doc[start..at]
}

const RUST: &str = "// hi\nfn main() {\n    let s = \"x\"; // done\n    if s.len() > 1 { return; }\n}";

#[test]
fn off_by_default_and_the_look_is_unchanged() {
    let doc = body_with(&fenced("rust", RUST), &Config::native_math());
    assert!(doc.contains("w:fill=\"F2F2F2\""));
    assert!(
        !doc.contains("w:color w:val="),
        "no colours without highlighting: {doc}"
    );
}

#[test]
fn light_theme_colours_tokens_and_sets_the_block_look() {
    let doc = body_with(&fenced("rust", RUST), &light());
    assert_well_formed(&doc);
    assert!(run_with(&doc, "fn").contains("w:val=\"0000FF\""), "keyword");
    assert!(run_with(&doc, "\"x\"").contains("w:val=\"A31515\""), "string");
    assert!(run_with(&doc, "// hi").contains("w:val=\"008000\""), "comment");
    assert!(run_with(&doc, "if").contains("w:val=\"AF00DB\""), "control keyword");
    assert!(run_with(&doc, "main").contains("w:val=\"795E26\""), "function");
    assert!(doc.contains("w:fill=\"FFFFFF\"") && !doc.contains("F2F2F2"));
    assert_eq!(copied_text(&doc), RUST);
}

#[test]
fn dark_theme_applies_to_every_code_block_even_without_a_language() {
    let md = "```rust\nfn f() {}\n```\n\n```\nplain\n```\n\n```brainfuck\n+++\n```\n\n    indented\n\n<pre>raw</pre>\n";
    let doc = body_with(md, &dark());
    assert_eq!(code_paragraphs(&doc).len(), 5);
    assert_eq!(doc.matches("w:fill=\"1E1E1E\"").count(), 5, "{doc}");
    assert!(!doc.contains("F2F2F2"));
    // Plain text is drawn in the theme's foreground so it stays readable on the dark fill.
    assert!(run_with(&doc, "plain").contains("w:val=\"D4D4D4\""));
    assert!(run_with(&doc, "+++").contains("w:val=\"D4D4D4\""));
    assert!(run_with(&doc, "fn").contains("w:val=\"569CD6\""));
}

#[test]
fn what_you_copy_is_what_was_written_for_every_language_and_layout() {
    let samples: &[(&str, &str)] = &[
        ("rust", RUST),
        ("python", "def f(x):\n\t'''doc\n\tmore'''  # c\n\treturn x  "),
        ("js", "const a = `x\n${y}`; /* multi\nline */ let z = 1;"),
        ("c", "#include <stdio.h>\n/* a\n   b */\nint main(){return 0;}"),
        ("sql", "SELECT 'it''s' -- c\nFROM t;"),
        ("json", "{\n  \"a\": [1, true, null]\n}"),
        ("yaml", "a: 1\nb:\n  - x\n# c"),
        (
            "toml",
            "[t]
k = \"v\"",
        ),
        ("html", "<a href=\"x\">\n  t &amp; u\n</a>"),
        ("css", "a { color: #fff; }"),
        ("diff", "--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y"),
        ("bash", "echo \"$HOME\" # c\nfor i in 1 2; do :; done"),
        ("rust", "é 日本 \u{1F600} \"é\" // ü"),
        ("rust", "a\r\nb // c\r\nd"),
    ];
    for layout in [CodeBlockLayout::LinePerParagraph, CodeBlockLayout::SingleParagraph] {
        for base in [light(), dark()] {
            let mut doc_opts = DocumentOptions::default();
            doc_opts.code_blocks = layout;
            let cfg = base.with_document(doc_opts);
            for (lang, code) in samples {
                let doc = body_with(&fenced(lang, code), &cfg);
                assert_well_formed(&doc);
                assert_only_xml_chars(&doc);
                let want = code.replace("\r\n", "\n");
                assert_eq!(copied_text(&doc), want.trim_end_matches('\n'), "[{lang}] {layout:?}");
            }
        }
    }
}

#[test]
fn multi_line_tokens_keep_their_colour_on_every_line() {
    let doc = body_with(&fenced("c", "/* one\ntwo\nthree */ int x;"), &light());
    for text in ["/* one", "two", "three */"] {
        assert!(
            run_with(&doc, text).contains("w:val=\"008000\""),
            "`{text}` lost the comment colour"
        );
    }
    assert!(run_with(&doc, "int").contains("w:val=\"267F99\"") || run_with(&doc, "int").contains("w:val=\"0000FF\""));
}

#[test]
fn language_comes_from_the_first_word_of_the_info_string() {
    for info in ["rust", "Rust", "RS", "rust,ignore", "rust title=\"x\"", "rust {1,3}"] {
        let doc = body_with(&fenced(info, "fn f() {}"), &light());
        assert!(run_with(&doc, "fn").contains("w:val=\"0000FF\""), "`{info}`");
    }
    for info in ["", "text", "rusty", "{.rust}"] {
        let doc = body_with(&fenced(info, "fn f() {}"), &light());
        assert!(!doc.contains("w:val=\"0000FF\""), "`{info}` should be plain");
    }
}

#[test]
fn single_paragraph_layout_is_coloured_too() {
    let mut opts = DocumentOptions::default();
    opts.code_blocks = CodeBlockLayout::SingleParagraph;
    let doc = body_with(&fenced("rust", RUST), &light().with_document(opts));
    assert_eq!(code_paragraphs(&doc).len(), 1);
    assert!(run_with(&doc, "fn").contains("w:val=\"0000FF\""));
    assert_eq!(doc.matches("<w:br").count(), RUST.lines().count() - 1);
}

#[test]
fn blocks_over_the_highlight_limit_are_drawn_plain_but_themed() {
    let mut cfg = light();
    cfg.limits.max_highlight_bytes = 8;
    let doc = body_with(&fenced("rust", "fn main() {}"), &cfg);
    assert!(!doc.contains("w:val=\"0000FF\""));
    assert!(doc.contains("w:fill=\"FFFFFF\""));
    assert_eq!(Config::untrusted().limits.max_highlight_bytes, 64 * 1024);
}

#[test]
fn hostile_code_cannot_break_the_xml() {
    let code = "</w:t></w:r></w:p><w:evil/> \"quote\" 'q' & <![CDATA[ ]]> \u{1}\u{7f} // </w:t>";
    for lang in ["rust", "html", "json", "sql", "bash", "css"] {
        let doc = body_with(&fenced(lang, &code.replace('\u{1}', "")), &dark());
        assert_well_formed(&doc);
        assert!(!doc.contains("<w:evil"), "[{lang}]");
    }
}

#[test]
fn it_works_in_the_untrusted_preset_and_counts_against_the_element_budget() {
    let cfg = Config::untrusted().with_highlighting(Highlighting::new(Theme::vscode_dark()));
    assert!(markdown_to_docx(&fenced("rust", RUST), &cfg).is_ok());
    // Highlighting makes many runs per line: the element limit still bounds the document.
    let busy = "a = 1; b = 2; c = 3;\n".repeat(200);
    let mut small = cfg.clone();
    small.limits.max_elements = 500;
    assert!(matches!(
        markdown_to_docx(&fenced("rust", &busy), &small),
        Err(CrateError::LimitExceeded(m)) if m.contains("max_elements")
    ));
}

// ---- custom highlighters

struct Todo;
impl Highlighter for Todo {
    fn highlight(&self, _lang: &str, source: &str) -> Result<Option<Vec<Span>>, FenceError> {
        Ok(Some(
            source
                .match_indices("TODO")
                .map(|(i, t)| Span::new(i, i + t.len(), TokenClass::Keyword))
                .collect(),
        ))
    }
}

#[test]
fn a_custom_highlighter_replaces_the_built_in_one() {
    let cfg = Config::trusted().with_highlighting(Highlighting::new(Theme::vscode_light()).with_highlighter(Todo));
    let doc = body_with(&fenced("whatever", "x TODO y"), &cfg);
    assert!(run_with(&doc, "TODO").contains("w:val=\"0000FF\""));
    // `fn` is not coloured because the built-in lexers are not consulted.
    let doc = body_with(&fenced("rust", "fn TODO"), &cfg);
    assert!(run_with(&doc, "fn ").contains("w:val=\"000000\""));
}

#[test]
fn per_language_highlighters_win_and_can_delegate_to_the_built_in_lexers() {
    let delegate = |lang: &str, src: &str| BuiltinHighlighter.highlight(lang, src);
    let h = Highlighting::new(Theme::vscode_light())
        .with_language("SQL", Todo)
        .with_highlighter(delegate);
    let cfg = Config::trusted().with_highlighting(h);
    let doc = body_with(&fenced("sql", "SELECT TODO"), &cfg);
    assert!(run_with(&doc, "TODO").contains("w:val=\"0000FF\""));
    assert!(!doc.contains(">SELECT</w:t>") || !run_with(&doc, "SELECT ").contains("w:val=\"0000FF\""));
    let doc = body_with(&fenced("rust", "fn f() {}"), &cfg);
    assert!(
        run_with(&doc, "fn").contains("w:val=\"0000FF\""),
        "delegation reached the built-in Rust lexer"
    );
    assert!(BuiltinHighlighter::supports("rs") && !BuiltinHighlighter::supports("cobol"));
}

#[test]
fn a_returned_none_means_plain() {
    let h = Highlighting::new(Theme::vscode_light())
        .with_highlighter(|_: &str, _: &str| -> Result<Option<Vec<Span>>, FenceError> { Ok(None) });
    let doc = body_with(&fenced("rust", "fn f() {}"), &Config::trusted().with_highlighting(h));
    assert!(!doc.contains("w:val=\"0000FF\"") && doc.contains("w:fill=\"FFFFFF\""));
}

#[test]
fn a_misbehaving_highlighter_is_an_error_not_a_corrupt_document() {
    let run = |f: Hl| {
        let h = Highlighting::new(Theme::vscode_light()).with_highlighter(f);
        markdown_to_docx(&fenced("rust", "fn é() {}"), &Config::trusted().with_highlighting(h))
    };
    let bad: &[Hl] = &[
        |_, _| Err("boom".into()),
        |_, _| panic!("highlighter bug"),
        |_, _| Ok(Some(vec![Span::new(0, 999, TokenClass::Keyword)])),
        |_, _| Ok(Some(vec![Span::new(3, 3, TokenClass::Keyword)])),
        |_, _| {
            Ok(Some(vec![
                Span::new(0, 3, TokenClass::Keyword),
                Span::new(2, 5, TokenClass::String),
            ]))
        },
        |_, _| {
            Ok(Some(vec![
                Span::new(3, 5, TokenClass::Keyword),
                Span::new(0, 2, TokenClass::String),
            ]))
        },
        |_, _| Ok(Some(vec![Span::new(0, 4, TokenClass::Keyword)])), // ends inside the two-byte é
    ];
    for (i, f) in bad.iter().enumerate() {
        match run(*f) {
            Err(CrateError::ExternalToolFailed { program, .. }) => assert!(program.contains("rust"), "{i}: {program}"),
            other => panic!(
                "case {i}: expected a highlighter error, got {:?}",
                other.map(|d| d.len())
            ),
        }
    }
}

#[test]
fn themes_and_languages_are_validated() {
    let bad_themes = [
        Theme::new("x", "FFF", "000000"),
        Theme::new("x", "FFFFFF", "zzzzzz"),
        Theme::new("x", "FFFFFF", "000000").style(TokenClass::Keyword, TokenStyle::color("red")),
        Theme::new("x\u{7}", "FFFFFF", "000000"),
    ];
    for t in bad_themes {
        let cfg = Config::trusted().with_highlighting(Highlighting::new(t));
        assert!(matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))));
    }
    let cfg =
        Config::trusted().with_highlighting(Highlighting::new(Theme::vscode_dark()).with_language("has space", Todo));
    assert!(matches!(markdown_to_docx("x", &cfg), Err(CrateError::InvalidConfig(_))));
    // A custom theme with bold and italic works.
    let theme = Theme::new("mine", "FAFAFA", "111111")
        .style(TokenClass::Keyword, TokenStyle::color("FF0000").bold().italic())
        .style(TokenClass::Comment, TokenStyle::default().italic());
    let doc = body_with(
        &fenced("rust", "fn f() {} // c"),
        &Config::trusted().with_highlighting(Highlighting::new(theme)),
    );
    let kw = run_with(&doc, "fn");
    assert!(
        kw.contains("w:val=\"FF0000\"") && kw.contains("<w:b/>") && kw.contains("<w:i/>"),
        "{kw}"
    );
    let c = run_with(&doc, "// c");
    assert!(c.contains("w:val=\"111111\"") && c.contains("<w:i/>"), "{c}");
}

#[test]
fn the_built_in_lexers_cover_the_documented_languages() {
    for lang in [
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
        "xml",
        "css",
        "diff",
        "rs",
        "py",
        "js",
        "ts",
        "sh",
        "yml",
        "c++",
    ] {
        assert!(BuiltinHighlighter::supports(lang), "{lang}");
    }
}
