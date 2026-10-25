//! Inputs that are valid but shaped to be expensive: very deep nesting, very wide
//! or tall tables, huge lines, long delimiter runs, quadratic-parsing traps, and
//! HTML that never closes.
//!
//! Each case runs on a normal test thread (2 MiB stack, so unbounded recursion
//! shows up as a crash) and must finish within a time budget with either a
//! well-formed document or one of the documented limit errors. A panic, an
//! overflow, a hang or any other error fails the test.

mod common;

use std::time::{Duration, Instant};

use common::*;
use silvercity_oss::{markdown_to_docx, Config, CrateError, ImageAccess};

/// Generous for an unoptimised build on a busy CI runner; a quadratic blow-up
/// on these sizes takes minutes.
const BUDGET: Duration = Duration::from_secs(40);

#[derive(Debug, PartialEq)]
enum Outcome {
    Document,
    Refused,
}

fn run(name: &str, md: &str, cfg: &Config) -> Outcome {
    let started = Instant::now();
    let result = markdown_to_docx(md, cfg);
    let took = started.elapsed();
    assert!(took < BUDGET, "[{name}] took {took:?} for {} input bytes", md.len());
    match result {
        Ok(docx) => {
            let doc = zip_text(&docx, "word/document.xml");
            assert_well_formed(&doc);
            assert_only_xml_chars(&doc);
            eprintln!("[{name}] ok in {took:?}: {} -> {} bytes", md.len(), docx.len());
            Outcome::Document
        }
        Err(CrateError::LimitExceeded(why)) => {
            eprintln!("[{name}] refused in {took:?}: {why}");
            Outcome::Refused
        }
        Err(other) => panic!("[{name}] unexpected error: {other}"),
    }
}

fn configs() -> [(&'static str, Config); 2] {
    [
        ("default", Config::native_math().with_image_access(ImageAccess::None)),
        ("untrusted", Config::untrusted()),
    ]
}

/// The input must convert in the default configuration. The untrusted preset may instead
/// refuse it with a limit (its element budget is deliberately small); either way it must
/// finish in time with a well-formed result or a `LimitExceeded`, and nothing else.
fn must_convert(name: &str, md: &str) {
    for (label, cfg) in configs() {
        let outcome = run(&format!("{name}/{label}"), md, &cfg);
        if label == "default" {
            assert_eq!(outcome, Outcome::Document, "[{name}/{label}] should convert");
        }
    }
}

/// The input may convert or be refused by a limit, but nothing else.
fn may_be_refused(name: &str, md: &str) {
    for (label, cfg) in configs() {
        run(&format!("{name}/{label}"), md, &cfg);
    }
}

// ---- nesting

#[test]
fn deeply_nested_lists() {
    let md: String = (0..1200).map(|i| format!("{}- item {i}\n", "  ".repeat(i))).collect();
    must_convert("nested bullets", &md);
    let md: String = (0..900).map(|i| format!("{}1. item {i}\n", "   ".repeat(i))).collect();
    must_convert("nested numbers", &md);
}

#[test]
fn deeply_nested_block_quotes() {
    must_convert("quote prefix", &format!("{}deep\n", "> ".repeat(5000)));
    let md: String = (1..400).map(|i| format!("{}line\n", "> ".repeat(i))).collect();
    must_convert("quote staircase", &md);
}

#[test]
fn alternating_containers() {
    // quote > list > quote > list ...
    let mut prefix = String::new();
    let mut md = String::new();
    for i in 0..300 {
        prefix.push_str(if i % 2 == 0 { "> " } else { "  - " });
        md.push_str(&format!("{prefix}level {i}\n"));
    }
    must_convert("quote/list alternation", &md);
    must_convert("list item with code", &"- a\n  ```\n  code\n  ```\n".repeat(2000));
}

#[test]
fn deeply_nested_inline_markup() {
    must_convert("emphasis openers", &"*a ".repeat(20_000));
    must_convert(
        "nested strong/em",
        &format!("{}x{}", "**_".repeat(5000), "_**".repeat(5000)),
    );
    must_convert("nested links", &format!("{}x{}", "[".repeat(5000), "](u)".repeat(5000)));
    must_convert(
        "nested images",
        &format!("{}x{}", "![".repeat(2000), "](u)".repeat(2000)),
    );
    must_convert("strikethrough runs", &"~~a ".repeat(20_000));
}

// ---- tables

#[test]
fn very_wide_table() {
    let cols = 1000;
    let row = |cell: &str| format!("|{}\n", format!(" {cell} |").repeat(cols));
    let sep = format!("|{}\n", "---|".repeat(cols));
    let md = format!("{}{sep}{}{}", row("h"), row("c"), row("d"));
    must_convert("1000 columns", &md);
}

#[test]
fn very_tall_table() {
    let mut md = String::from("| a | b |\n|---|---|\n");
    for i in 0..20_000 {
        md.push_str(&format!("| {i} | x |\n"));
    }
    must_convert("20000 rows", &md);
}

#[test]
fn table_cells_with_heavy_content() {
    let cell = "**b** `c` [l](https://example.com) ".repeat(500);
    let md = format!("| a | b |\n|---|---|\n| {cell} | {cell} |\n");
    must_convert("heavy cells", &md);
    must_convert("table of tables", &"<table><tr><td>".repeat(3000));
}

// ---- size

#[test]
fn very_long_lines() {
    must_convert("one 1.5 MiB word", &"x".repeat(3 << 19));
    must_convert("one 1.5 MiB line of words", &"word ".repeat(300_000));
    // Beyond the untrusted input cap: the default configuration still copes.
    let big = "x".repeat(8 << 20);
    run("one 8 MiB word/default", &big, &configs()[0].1);
    assert!(matches!(
        markdown_to_docx(&big, &Config::untrusted()),
        Err(CrateError::LimitExceeded(_))
    ));
    must_convert("long heading", &format!("# {}\n", "h ".repeat(200_000)));
    must_convert("long code line", &format!("```\n{}\n```\n", "c".repeat(3 << 19)));
    must_convert(
        "long link text and url",
        &format!("[{}](https://e.com/{})", "t".repeat(700_000), "u".repeat(700_000)),
    );
}

#[test]
fn many_small_blocks() {
    must_convert("30k paragraphs", &"p\n\n".repeat(30_000));
    must_convert("30k list items", &"- i\n".repeat(30_000));
    must_convert("30k rules", &"---\n\n".repeat(30_000));
    must_convert("60k code lines", &format!("```\n{}```\n", "line\n".repeat(60_000)));
    must_convert("100k blank lines", &"\n".repeat(100_000));
    must_convert("30k headings", &"# h\n".repeat(30_000));
}

// ---- parsing traps

#[test]
fn delimiter_runs() {
    for (name, unit) in [
        ("asterisks", "*"),
        ("underscores", "_"),
        ("backticks", "`"),
        ("tildes", "~"),
        ("open brackets", "["),
        ("close brackets", "]"),
        ("open image brackets", "!["),
        ("angle brackets", "<"),
        ("ampersands", "&"),
        ("backslashes", "\\"),
        ("pipes", "|"),
        ("hashes", "#"),
        ("colons", ":"),
        ("carets", "^"),
    ] {
        must_convert(name, &unit.repeat(200_000));
    }
    // Every `$$` pair is a formula, so this is refused by the formula limit.
    assert_eq!(
        run("dollars/default", &"$".repeat(200_000), &configs()[0].1),
        Outcome::Refused
    );
}

#[test]
fn quadratic_parsing_traps() {
    // Patterns known to cost O(n^2) in naive CommonMark implementations.
    must_convert("emphasis ladder", &"*a **a ".repeat(25_000));
    must_convert(
        "unmatched backtick runs",
        &(1..600).map(|n| "`".repeat(n) + " ").collect::<String>(),
    );
    must_convert("link openers", &"[a](".repeat(40_000));
    must_convert("reference openers", &"[a][".repeat(40_000));
    must_convert("autolink openers", &"<http://a ".repeat(30_000));
    must_convert("html comment openers", &"<!-- ".repeat(40_000));
    must_convert("entities", &"&#35;&amp;&copy;".repeat(40_000));
}

#[test]
fn many_references() {
    let mut md = String::new();
    for i in 0..30_000 {
        md.push_str(&format!("[r{i}]: https://example.com/{i}\n"));
    }
    md.push('\n');
    for i in 0..30_000 {
        md.push_str(&format!("[r{i}] "));
    }
    must_convert("30k reference links", &md);
    must_convert(
        "one link, 50k uses",
        &format!("[x]: https://e.com/\n\n{}", "[x] ".repeat(50_000)),
    );
}

#[test]
fn footnote_amplification() {
    let body = "text ".repeat(2000);
    let refs = "[^a] ".repeat(10_000);
    may_be_refused("one big note, 10k references", &format!("{refs}\n\n[^a]: {body}\n"));
    let mut md = String::new();
    for i in 0..5000 {
        md.push_str(&format!("see[^{i}] "));
    }
    md.push_str("\n\n");
    for i in 0..5000 {
        md.push_str(&format!("[^{i}]: note {i}\n\n"));
    }
    may_be_refused("5000 notes", &md);
}

// ---- raw HTML that never closes

#[test]
fn unclosed_and_runaway_html() {
    must_convert("unclosed divs", &"<div>".repeat(50_000));
    must_convert("unclosed bold", &"<b>x".repeat(50_000));
    must_convert("unclosed links", &"<a href=\"https://example.com/\">x".repeat(20_000));
    must_convert("unclosed lists", &"<ul><li>x".repeat(5000));
    must_convert("unclosed ordered", &"<ol start=\"1\"><li>x".repeat(5000));
    must_convert("unclosed pre", &"<pre>x\n".repeat(20_000));
    must_convert("unclosed blockquote", &"<blockquote>x".repeat(20_000));
    must_convert("unclosed script", &format!("<script>{}", "a".repeat(1 << 20)));
    must_convert(
        "stray close tags",
        &"</b></div></td></tr></table></a></li>".repeat(20_000),
    );
    must_convert("unterminated tag", &format!("<div {}", "a=b ".repeat(200_000)));
    must_convert("huge attribute", &format!("<img src=\"{}\">", "a".repeat(1 << 20)));
    must_convert(
        "many attributes",
        &format!("<p {}>x</p>", "align=left ".repeat(100_000)),
    );
    must_convert(
        "nested tables",
        &format!(
            "{}x{}",
            "<table><tr><td>".repeat(2000),
            "</td></tr></table>".repeat(2000)
        ),
    );
}

// ---- text

#[test]
fn unusual_text() {
    must_convert("combining marks", &format!("e{}", "\u{0301}".repeat(500_000)));
    must_convert("right-to-left", &"\u{05D0}\u{05D1}\u{05D2} ".repeat(100_000));
    must_convert("bidi controls", &"\u{202E}a\u{202C}".repeat(100_000));
    must_convert("emoji", &"\u{1F600}\u{200D}\u{1F468}".repeat(100_000));
    must_convert(
        "replacement and private use",
        &"\u{FFFD}\u{E000}\u{10FFFF}".repeat(100_000),
    );
    must_convert("carriage returns only", &"a\rb\r".repeat(100_000));
    must_convert("mixed line endings", &"a\r\nb\nc\rd\n".repeat(50_000));
    must_convert("tabs", &"\t".repeat(200_000));
    must_convert("zero width", &"a\u{200B}".repeat(200_000));
}

#[test]
fn math_flood() {
    // More than `max_rendered_blocks` formulas is refused before any work is done.
    for (name, md) in [
        ("100k inline formulas", "$x$ ".repeat(100_000)),
        ("50k display formulas", "$$x$$\n\n".repeat(50_000)),
    ] {
        for (label, cfg) in configs() {
            assert_eq!(run(&format!("{name}/{label}"), &md, &cfg), Outcome::Refused);
        }
    }
    // Just under the limit converts.
    must_convert("150 formulas", &"$x$ ".repeat(150));
    // Too deeply nested (or too large) to convert: a clean error, not a crash.
    let deep = format!("${}x{}$", "{".repeat(5000), "}".repeat(5000));
    for (_, cfg) in configs() {
        // Untrusted limits reject the formula by size before nesting is even looked at.
        assert!(matches!(
            markdown_to_docx(&deep, &cfg),
            Err(CrateError::InvalidMarkdown(_) | CrateError::LimitExceeded(_))
        ));
    }
}
