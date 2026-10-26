//! A coverage matrix of the Markdown found in real files: CommonMark, GitHub
//! extensions, raw HTML, and the awkward inputs around them. Each case states
//! what must appear in the Word document and what must not.

mod common;

use common::*;
use md2docx::{markdown_to_docx, Config};

/// `(name, markdown, must contain in document.xml, visible text must contain, must NOT appear in visible text)`
type Case = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

fn run(cases: &[Case]) {
    for (name, md, xml_has, text_has, text_lacks) in cases {
        let b = body(md);
        let text = visible_text(&b);
        for want in *xml_has {
            assert!(b.contains(want), "[{name}] document.xml lacks `{want}`\n{b}");
        }
        for want in *text_has {
            assert!(text.contains(want), "[{name}] text lacks `{want}`; got: {text}");
        }
        for bad in *text_lacks {
            assert!(
                !text.contains(bad),
                "[{name}] text should not contain `{bad}`; got: {text}"
            );
        }
    }
}

#[test]
fn headings() {
    run(&[
        (
            "atx 1-6",
            "# a\n## b\n### c\n#### d\n##### e\n###### f\n",
            &["Heading1", "Heading2", "Heading3", "Heading4", "Heading5", "Heading6"],
            &["a", "f"],
            &["#"],
        ),
        (
            "setext",
            "Title\n=====\n\nSub\n---\n",
            &["Heading1", "Heading2"],
            &["Title", "Sub"],
            &["===", "---"],
        ),
        ("closing hashes", "## Closed ##\n", &["Heading2"], &["Closed"], &["##"]),
        (
            "with attributes",
            "# Title {#id .cls key=val}\n",
            &["Heading1"],
            &["Title"],
            &["{#id", ".cls"],
        ),
        (
            "inline formatting",
            "# A *b* **c** `d`\n",
            &["Heading1", "<w:i/>", "<w:b/>"],
            &["A b c d"],
            &[],
        ),
        (
            "not a heading",
            "#hashtag and \\# escaped\n",
            &[],
            &["#hashtag", "# escaped"],
            &[],
        ),
    ]);
}

#[test]
fn inline_formatting() {
    run(&[
        (
            "emphasis",
            "*a* _b_ **c** __d__ ***e*** ~~f~~\n",
            &["<w:i/>", "<w:b/>", "<w:strike/>"],
            &["a b c d e f"],
            &["*", "~~"],
        ),
        (
            "intraword",
            "snake_case_name and 2*3*4\n",
            &[],
            &["snake_case_name", "2"],
            &[],
        ),
        (
            "code spans",
            "`a` and ``b ` c`` and ` d `\n",
            &["InlineCode"],
            &["a", "b ` c"],
            &[],
        ),
        (
            "escapes",
            "\\*not italic\\* \\_x\\_ \\[no link\\] \\\\ \\`tick\n",
            &[],
            &["*not italic*", "_x_", "[no link]", "\\", "`tick"],
            &[],
        ),
        (
            "entities",
            "&copy; &amp; &lt;b&gt; &#65; &#x42; &nbsp;|\n",
            &[],
            &["©", "& <b> A B"],
            &["&amp;", "&copy;"],
        ),
        (
            "hard breaks",
            "two spaces  \nnext\\\nbackslash\n",
            &["<w:br "],
            &["two spaces", "next", "backslash"],
            &[],
        ),
        ("soft break", "one\ntwo\n", &[], &["one two"], &[]),
        (
            "unicode",
            "日本語 😀 Ελληνικά עברית العربية é\u{301}\n",
            &[],
            &["日本語 😀 Ελληνικά עברית العربية é\u{301}"],
            &[],
        ),
        (
            "dollar amounts",
            "It costs $5 and $10, or $1.50.\n",
            &[],
            &["$5 and $10, or $1.50."],
            &[],
        ),
    ]);
}

#[test]
fn links_and_images() {
    let b = body("[inline](https://a.example \"title\") [ref][r] [shortcut] <https://auto.example> <me@example.com>\n\n[r]: https://ref.example\n[shortcut]: https://short.example\n");
    assert_eq!(b.matches("<w:hyperlink").count(), 5, "{b}");
    run(&[
        (
            "link with formatting",
            "[**bold** `code`](https://x.example)\n",
            &["<w:hyperlink", "<w:b/>"],
            &["bold code"],
            &[],
        ),
        (
            "image title ignored",
            "![alt text](https://img.example/a.png \"title\")\n",
            &[],
            &["[alt text]"],
            &[],
        ),
        (
            "badge: image inside link",
            "[![build](https://img.example/b.svg)](https://ci.example)\n",
            &["<w:hyperlink"],
            &["[build]"],
            &[],
        ),
        ("empty link text", "[](https://empty.example)\n", &[], &[], &[]),
        (
            "bare url stays text",
            "see https://bare.example/path now\n",
            &[],
            &["https://bare.example/path"],
            &[],
        ),
        ("unbalanced brackets", "[a](b [c] ![d] ](\n", &[], &["[c]"], &[]),
    ]);
}

#[test]
fn lists() {
    // Numbering definitions: ordered lists restart, start numbers are honoured.
    let docx = markdown_to_docx(
        "1. a\n2. b\n\ntext\n\n7. seven\n8. eight\n\n- x\n  1. nested\n  2. again\n- y\n",
        &Config::native_math(),
    )
    .unwrap();
    let numbering = zip_text(&docx, "word/numbering.xml");
    assert!(
        numbering.contains("w:startOverride w:val=\"7\""),
        "start number 7 missing:\n{numbering}"
    );
    let b = zip_text(&docx, "word/document.xml");
    assert_well_formed(&b);
    assert!(b.matches("<w:numPr>").count() >= 7);

    run(&[
        (
            "tight bullets",
            "- a\n- b\n* c\n+ d\n",
            &["<w:numPr>"],
            &["a", "b", "c", "d"],
            &["- a", "* c", "+ d"],
        ),
        (
            "loose list",
            "- a\n\n- b\n\n  second para of b\n",
            &["<w:numPr>"],
            &["a", "b", "second para of b"],
            &[],
        ),
        (
            "task list",
            "- [x] done\n- [ ] todo\n",
            &[],
            &["☑ done", "☐ todo"],
            &["[x]", "[ ]"],
        ),
        (
            "three levels",
            "- a\n  - b\n    - c\n      - d\n",
            &["w:ilvl w:val=\"3\""],
            &["d"],
            &[],
        ),
        (
            "code in item",
            "- item\n\n  ```rust\n  let x = 1;\n  ```\n",
            &["CodeBlock"],
            &["let x = 1;"],
            &["```"],
        ),
        ("quote in item", "- item\n\n  > quoted\n", &[], &["quoted"], &[]),
        (
            "table in item",
            "- item\n\n  | h |\n  |---|\n  | v |\n",
            &["<w:tbl>"],
            &["v"],
            &[],
        ),
        ("empty item", "-\n- second\n", &[], &["second"], &[]),
        (
            "ordered with paren",
            "1) one\n2) two\n",
            &["<w:numPr>"],
            &["one", "two"],
            &[],
        ),
        ("huge start number", "99999999999999999999. big\n", &[], &[], &[]),
    ]);
}

#[test]
fn block_quotes_and_alerts() {
    run(&[
        ("quote", "> a\n> b\n", &["w:left"], &["a b"], &[">"]),
        (
            "nested",
            "> a\n>\n> > b\n>\n> > > c\n",
            &["w:left"],
            &["a", "b", "c"],
            &[],
        ),
        ("lazy continuation", "> a\nlazy\n", &[], &["a lazy"], &[]),
        (
            "quote with list and code",
            "> - x\n> - y\n>\n> ```\n> code\n> ```\n",
            &["CodeBlock"],
            &["x", "y", "code"],
            &[],
        ),
        ("note", "> [!NOTE]\n> text\n", &[], &["Note", "text"], &["[!NOTE]"]),
        ("tip", "> [!TIP]\n> text\n", &[], &["Tip"], &["[!TIP]"]),
        ("important", "> [!IMPORTANT]\n> text\n", &[], &["Important"], &[]),
        ("warning", "> [!WARNING]\n> text\n", &[], &["Warning"], &[]),
        ("caution", "> [!CAUTION]\n> text\n", &[], &["Caution"], &[]),
    ]);
}

#[test]
fn code() {
    run(&[
        (
            "fenced",
            "```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n",
            &["CodeBlock"],
            &["fn main() {", "println!"],
            &["```"],
        ),
        ("tilde fence", "~~~\nplain\n~~~\n", &["CodeBlock"], &["plain"], &["~~~"]),
        (
            "indented",
            "    indented code\n    more\n",
            &["CodeBlock"],
            &["indented code", "more"],
            &[],
        ),
        (
            "longer fence containing fence",
            "````\n```\ninner\n```\n````\n",
            &["CodeBlock"],
            &["```", "inner"],
            &[],
        ),
        (
            "unclosed fence",
            "```\nnever closed\n",
            &["CodeBlock"],
            &["never closed"],
            &[],
        ),
        ("empty fence", "```\n```\n", &[], &[], &[]),
        (
            "code with markup chars",
            "```\n<w:p> & \"q\" ]]>\n```\n",
            &["&lt;w:p&gt;"],
            &["<w:p> & \"q\" ]]>"],
            &[],
        ),
        (
            "crlf",
            "```\r\nline1\r\nline2\r\n```\r\n",
            &["CodeBlock"],
            &["line1", "line2"],
            &["\r"],
        ),
        (
            "tabs preserved",
            "```\n\tindented\n```\n",
            &["CodeBlock"],
            &["indented"],
            &[],
        ),
    ]);
}

#[test]
fn tables() {
    run(&[
        (
            "alignments",
            "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n",
            &["<w:tbl>", "w:jc w:val=\"center\"", "w:jc w:val=\"right\""],
            &["L", "C", "R", "a", "b", "c"],
            &["|"],
        ),
        (
            "inline formatting",
            "| h |\n|---|\n| **b** `c` [l](https://t.example) |\n",
            &["<w:tbl>", "<w:hyperlink", "InlineCode"],
            &["b c l"],
            &[],
        ),
        (
            "escaped pipe",
            "| a |\n|---|\n| x \\| y |\n",
            &["<w:tbl>"],
            &["x | y"],
            &[],
        ),
        (
            "empty cells",
            "| a | b |\n|---|---|\n|   | z |\n",
            &["<w:tbl>"],
            &["z"],
            &[],
        ),
        (
            "ragged rows",
            "| a | b | c |\n|---|---|---|\n| 1 |\n| 1 | 2 | 3 | 4 |\n",
            &["<w:tbl>"],
            &["1"],
            &[],
        ),
        (
            "header only",
            "| only | header |\n|------|--------|\n",
            &["<w:tbl>"],
            &["only", "header"],
            &[],
        ),
        (
            "no leading pipes",
            "a | b\n--|--\n1 | 2\n",
            &["<w:tbl>"],
            &["a", "2"],
            &[],
        ),
        (
            "math in cell",
            "| f |\n|---|\n| $x^2$ |\n",
            &["<w:tbl>", "<m:oMath>"],
            &[],
            &["$"],
        ),
        ("not a table", "a | b\nno delimiter row\n", &[], &["a | b"], &[]),
    ]);
}

#[test]
fn rules_and_breaks() {
    let b = body("a\n\n---\n\nb\n\n***\n\nc\n\n___\n\nd\n");
    assert_eq!(b.matches("<w:pBdr>").count(), 3, "three rules");
    run(&[
        (
            "rule between text",
            "above\n\n- - -\n\nbelow\n",
            &["w:bottom"],
            &["above", "below"],
            &["- - -"],
        ),
        (
            "two rules around content stays content",
            "---\nThis is content between rules\n---\n",
            &[],
            &["This is content between rules"],
            &[],
        ),
        (
            "setext is not a rule",
            "Heading\n---\n",
            &["Heading2"],
            &["Heading"],
            &[],
        ),
    ]);
}

#[test]
fn front_matter() {
    run(&[
        (
            "yaml",
            "---\ntitle: Secret meta\ntags: [a, b]\n---\n\n# Body\n",
            &["Heading1"],
            &["Body"],
            &["Secret meta", "tags"],
        ),
        (
            "toml",
            "+++\ntitle = \"Secret meta\"\n+++\n\nBody text\n",
            &[],
            &["Body text"],
            &["Secret meta"],
        ),
        ("only front matter", "---\ntitle: x\n---\n", &[], &[], &["title"]),
        (
            "with bom",
            "\u{feff}---\ntitle: x\n---\nBody\n",
            &[],
            &["Body"],
            &["title"],
        ),
        (
            "windows newlines",
            "---\r\ntitle: x\r\n---\r\nBody\r\n",
            &[],
            &["Body"],
            &["title"],
        ),
        (
            "mid-document metadata-looking block is content",
            "Intro\n\n---\nkey: value\n---\n\nAfter\n",
            &[],
            &["Intro", "key: value", "After"],
            &[],
        ),
        (
            "rule-delimited paragraph is content",
            "---\nJust prose here.\n---\n\nMore\n",
            &[],
            &["Just prose here.", "More"],
            &[],
        ),
    ]);
}

#[test]
fn raw_html() {
    run(&[
        (
            "inline styles",
            "a <b>bold</b> <i>it</i> <u>un</u> <s>st</s> <sup>up</sup> <sub>dn</sub> <code>mono</code> <kbd>K</kbd>\n",
            &[
                "<w:b/>",
                "<w:i/>",
                "<w:u ",
                "<w:strike/>",
                "w:vertAlign w:val=\"superscript\"",
                "w:vertAlign w:val=\"subscript\"",
                "InlineCode",
            ],
            &["bold", "it", "un", "st", "up", "dn", "mono", "K"],
            &["<b>", "</b>"],
        ),
        ("line break", "a<br>b<br/>c<br />d\n", &["<w:br "], &["abcd"], &["<br"]),
        (
            "block with text",
            "<div>only html text</div>\n\nafter\n",
            &[],
            &["only html text", "after"],
            &["<div>"],
        ),
        (
            "block with image tag, no access",
            "<p align=\"center\"><img src=\"https://img.example/logo.png\" alt=\"Logo\" width=\"100\"></p>\n",
            &[],
            &[],
            &["<img"],
        ),
        (
            "headings",
            "<h2>HTML two</h2>\n\n<h4>HTML four</h4>\n",
            &["Heading2", "Heading4"],
            &["HTML two", "HTML four"],
            &["<h2>"],
        ),
        (
            "lists",
            "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n",
            &["<w:numPr>"],
            &["one", "two"],
            &["<li>", "\u{2022}"],
        ),
        (
            "table text",
            "<table><tr><td>cell a</td><td>cell b</td></tr></table>\n",
            &[],
            &["cell a", "cell b"],
            &["<td>"],
        ),
        (
            "entities in html",
            "<p>fish &amp; chips &lt;3 &copy;</p>\n",
            &[],
            &["fish & chips <3"],
            &["&amp;"],
        ),
        (
            "comment",
            "<!-- hidden note -->\n\nvisible\n",
            &[],
            &["visible"],
            &["hidden note"],
        ),
        (
            "script and style dropped",
            "<script>alert('x')</script>\n\n<style>p{color:red}</style>\n\ntext\n",
            &[],
            &["text"],
            &["alert", "color"],
        ),
        (
            "iframe dropped",
            "<iframe src=\"https://evil.example\">inner</iframe>\n\nok\n",
            &[],
            &["ok"],
            &["evil", "inner"],
        ),
        (
            "details/summary",
            "<details>\n<summary>Click</summary>\n\nHidden *content*\n\n</details>\n",
            &[],
            &["Click", "Hidden content"],
            &["<details>"],
        ),
        (
            "unknown tags keep text",
            "<custom-tag attr=1>kept</custom-tag> and <span style=\"x\">span</span>\n",
            &[],
            &["kept", "span"],
            &["<span"],
        ),
        (
            "stray angle brackets",
            "1 < 2 and 3 > 2, a<b>c\n",
            &[],
            &["1 < 2 and 3 > 2"],
            &[],
        ),
        ("html hr", "<hr>\n\nafter\n", &["w:bottom"], &["after"], &[]),
        (
            "doctype/xml decl",
            "<!DOCTYPE html>\n\n<?xml version=\"1.0\"?>\n\nhi\n",
            &[],
            &["hi"],
            &["DOCTYPE"],
        ),
    ]);
}

#[test]
fn footnotes() {
    let docx = markdown_to_docx(
        "One[^a] two[^b] three[^a] and[^missing].\n\n[^a]: Note *A* with `code`.\n\n    Second paragraph of A.\n[^b]: Note B.\n",
        &Config::native_math(),
    )
    .unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert_eq!(
        body.matches("<w:footnoteReference").count(),
        3,
        "two refs to a, one to b"
    );
    let notes = zip_text(&docx, "word/footnotes.xml");
    assert_well_formed(&notes);
    let text = visible_text(&notes);
    assert!(
        text.contains("Note A") && text.contains("Second paragraph of A.") && text.contains("Note B."),
        "{text}"
    );
    let main = visible_text(&body);
    assert!(main.contains("[^missing]"), "unknown label stays literal: {main}");
    assert!(!main.contains("Note B."), "definitions are not repeated in the body");
}

#[test]
fn definition_lists() {
    run(&[
        (
            "basic",
            "Term\n: Definition one\n: Definition two\n\nOther\n: Its definition\n",
            &["w:ind"],
            &["Term", "Definition one", "Definition two", "Other", "Its definition"],
            &[": Definition"],
        ),
        (
            "formatted",
            "**Bold term**\n: with *emphasis*\n",
            &[],
            &["Bold term", "with emphasis"],
            &[],
        ),
    ]);
}

#[test]
fn math_in_context() {
    run(&[
        (
            "inline and display",
            "Inline $a^2$ and\n\n$$\\frac{a}{b}$$\n",
            &["<m:sSup>", "<m:oMathPara>", "<m:f>"],
            &["Inline", "and"],
            &["$"],
        ),
        (
            "in heading",
            "# Title $E=mc^2$\n",
            &["Heading1", "<m:oMath>"],
            &["Title"],
            &[],
        ),
        ("in list", "- item $x_i$\n", &["<w:numPr>", "<m:sSub>"], &["item"], &[]),
        ("in quote", "> quoted $\\alpha$\n", &["<m:oMath>"], &["quoted"], &[]),
        (
            "in link text",
            "[$x$ link](https://m.example)\n",
            &["<w:hyperlink"],
            &[],
            &[],
        ),
        (
            "in footnote falls back to source",
            "ref[^1]\n\n[^1]: has $y^2$ inside\n",
            &[],
            &["ref"],
            &[],
        ),
        ("escaped dollar", "a \\$b and \\$c\n", &[], &["$b and $c"], &[]),
        (
            "dollar in code",
            "`$x$` and\n\n```\n$$y$$\n```\n",
            &["InlineCode", "CodeBlock"],
            &["$x$", "$$y$$"],
            &[],
        ),
        (
            "multiline display",
            "$$\n\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}\n$$\n",
            &["<m:eqArr>"],
            &[],
            &[],
        ),
    ]);
}

#[test]
fn degenerate_documents() {
    for md in [
        "",
        "\n",
        "   \n\t\n",
        "---\ntitle: x\n---\n",
        "\u{feff}",
        "\r\n\r\n",
        "<!-- only a comment -->",
        "[^1]: orphan definition",
        "[ref]: https://only-a-definition.example",
    ] {
        let docx = markdown_to_docx(md, &Config::native_math()).unwrap_or_else(|e| panic!("{md:?}: {e}"));
        let b = zip_text(&docx, "word/document.xml");
        assert_well_formed(&b);
        assert!(b.contains("<w:body>"));
        assert_eq!(&docx[..2], b"PK");
    }
}

#[test]
fn large_but_legitimate_documents_are_fast() {
    let mut md = String::new();
    for i in 0..4000 {
        md.push_str(&format!("## Section {i}\n\nParagraph with **bold**, *italic*, `code`, a [link](https://example.com/{i}) and math $x_{i}^2$.\n\n- item a\n- item b\n\n"));
    }
    let table: String = (0..1500).map(|i| format!("| {i} | cell {i} | **b** |\n")).collect();
    md.push_str(&format!("| n | text | f |\n|---|------|---|\n{table}"));
    let started = std::time::Instant::now();
    let mut limits = md2docx::Limits::trusted();
    limits.max_rendered_blocks = 10_000;
    let cfg = Config::native_math().with_limits(limits);
    let docx = markdown_to_docx(&md, &cfg).unwrap();
    let elapsed = started.elapsed();
    assert!(docx.len() > 100_000);
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "took {elapsed:?} for {} bytes of Markdown",
        md.len()
    );
    eprintln!(
        "{} bytes of Markdown -> {} byte docx in {elapsed:?}",
        md.len(),
        docx.len()
    );
}

#[test]
fn hidden_html_elements_split_across_inline_events_stay_hidden() {
    run(&[
        (
            "inline script",
            "before <script>alert(1)</script> after\n",
            &[],
            &["before", "after"],
            &["alert"],
        ),
        (
            "inline style",
            "a <style>.x{color:red}</style> b\n",
            &[],
            &["a", "b"],
            &["color"],
        ),
        (
            "unclosed script is bounded by the paragraph",
            "x <script>evil()\n\nnext paragraph survives\n",
            &[],
            &["next paragraph survives"],
            &["evil"],
        ),
        (
            "unclosed in a list item",
            "- a <style>p{}\n- b stays\n",
            &[],
            &["b stays"],
            &["p{}"],
        ),
        (
            "emphasis around hidden text",
            "*em <script>bad()</script> still em*\n",
            &["<w:i/>"],
            &["em", "still em"],
            &["bad"],
        ),
    ]);
}

/// Raw HTML is an allowlist: only the presentation tags listed in the README have an effect,
/// everything else contributes at most its text, and attributes are never interpreted
/// (`onclick`, `style`, `href`, `srcdoc` ...), so nothing active can reach the document.
#[test]
fn unsafe_html_is_stripped_but_its_text_is_kept() {
    let md = "\
<a href=\"javascript:alert(1)\" onclick=\"steal()\">click me</a> and <a href=\"https://ok.example\">plain anchor</a>\n\n\
<img src=x onerror=\"pwn()\"> <img src=\"javascript:evil()\" alt=\"bad image\">\n\n\
<svg onload=\"pwn()\"><script>bad()</script><text>vector text</text></svg>\n\n\
<form action=\"http://evil.example/post\"><input value=\"typed\"><button onclick=\"x()\">Go</button></form>\n\n\
<object data=\"http://evil.example/o\"></object><embed src=\"http://evil.example/e\">\n\n\
<link rel=\"stylesheet\" href=\"http://evil.example/c.css\"><base href=\"http://evil.example/\">\n\n\
<meta http-equiv=\"refresh\" content=\"0;url=http://evil.example\">\n\n\
<div style=\"background:url(javascript:1)\" onmouseover=\"x()\" class=\"c\">styled text</div>\n\n\
<iframe srcdoc=\"<script>top.location='http://evil.example'</script>\">frame body</iframe>\n\n\
<p><b onclick=\"x()\">still bold</b> and <i>italic</i></p>\n\n\
<math><mi>x</mi></math> <details open ontoggle=\"x()\"><summary>sum</summary>body</details>\n";
    for (name, cfg) in [
        (
            "default, images off",
            md2docx::Config::native_math().with_image_access(md2docx::ImageAccess::None),
        ),
        ("untrusted", md2docx::Config::untrusted()),
    ] {
        let docx = markdown_to_docx(md, &cfg).unwrap_or_else(|e| panic!("{name}: {e}"));
        let b = zip_text(&docx, "word/document.xml");
        assert_well_formed(&b);
        let rels = zip_text(&docx, "word/_rels/document.xml.rels");
        let text = visible_text(&b);

        for dangerous in [
            "javascript",
            "onclick",
            "onerror",
            "onload",
            "onmouseover",
            "ontoggle",
            "srcdoc",
            "evil",
            "steal",
            "pwn",
            "alert",
            "bad()",
            "stylesheet",
            "refresh",
        ] {
            assert!(!b.contains(dangerous), "[{name}] document.xml contains `{dangerous}`");
            assert!(
                !rels.contains(dangerous),
                "[{name}] relationships contain `{dangerous}`"
            );
        }
        // `<a href>` is the one place the modes differ: a safe https link becomes a hyperlink
        // by default, never in --untrusted; the javascript: one is never a link.
        let expected_links = usize::from(name != "untrusted");
        assert_eq!(b.matches("<w:hyperlink").count(), expected_links, "[{name}]");
        assert_eq!(rels.contains("https://ok.example"), expected_links == 1, "[{name}]");
        assert_eq!(media_count(&docx), 0, "[{name}]");
        for kept in [
            "click me",
            "plain anchor",
            "bad image",
            "vector text",
            "Go",
            "styled text",
            "still bold",
            "italic",
            "sum",
            "body",
        ] {
            assert!(text.contains(kept), "[{name}] text `{kept}` was lost: {text}");
        }
        assert!(!text.contains("frame body"), "[{name}] iframe content is dropped");
        assert!(
            b.contains("<w:b/>") && b.contains("<w:i/>"),
            "[{name}] harmless formatting tags still work"
        );
    }
}

// ---- raw HTML the way people write it in READMEs (default mode; --untrusted differs only in links)

fn html_docx(md: &str, cfg: &md2docx::Config) -> (Vec<u8>, String) {
    let docx = markdown_to_docx(md, cfg).expect("conversion");
    let b = zip_text(&docx, "word/document.xml");
    assert_well_formed(&b);
    (docx, b)
}

fn no_images() -> md2docx::Config {
    md2docx::Config::native_math().with_image_access(md2docx::ImageAccess::None)
}

#[test]
fn html_links_become_hyperlinks_unless_untrusted() {
    let md = "Visit <a href=\"https://example.com/docs\">the docs</a>, <a href=\"mailto:me@example.com\">mail</a>, \
              <a href=\"javascript:alert(1)\">bad</a>, <a href=\"file:///etc/passwd\">file</a>, <a href=\"#top\">anchor</a>, <a>no href</a>.\n\n\
              <p align=\"center\"><a href=\"https://ci.example\"><img src=\"https://img.example/badge.svg\" alt=\"build\"></a></p>\n";
    let (docx, b) = html_docx(md, &no_images());
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert_eq!(b.matches("<w:hyperlink").count(), 3, "docs, mail and the badge: {b}");
    for good in [
        "https://example.com/docs",
        "mailto:me@example.com",
        "https://ci.example",
    ] {
        assert!(rels.contains(good), "{good} missing");
    }
    for bad in ["javascript", "file:", "#top"] {
        assert!(!rels.contains(bad), "{bad} became a link target");
    }
    let text = visible_text(&b);
    for label in ["the docs", "mail", "bad", "file", "anchor", "no href", "[build]"] {
        assert!(text.contains(label), "`{label}` lost: {text}");
    }

    // The strict preset keeps the text and drops every link.
    let (docx, b) = html_docx(md, &md2docx::Config::untrusted());
    assert_eq!(b.matches("<w:hyperlink").count(), 0);
    assert!(!zip_text(&docx, "word/_rels/document.xml.rels").contains("example.com"));
    assert!(visible_text(&b).contains("the docs"));
}

#[test]
fn unclosed_or_overlapping_html_links_are_contained() {
    let (_, b) = html_docx(
        "a <a href=\"https://x.example\">unclosed link\n\nnext paragraph <a href=\"https://y.example\">also open",
        &no_images(),
    );
    assert_eq!(b.matches("<w:hyperlink").count(), 2, "each ends with its paragraph");
    assert!(visible_text(&b).contains("next paragraph"));
    // An HTML link inside a Markdown link does not nest or corrupt anything.
    let (_, b) = html_docx(
        "[md <a href=\"https://inner.example\">html</a> link](https://outer.example)\n",
        &no_images(),
    );
    assert!(b.matches("<w:hyperlink").count() >= 1);
    assert!(visible_text(&b).contains("html"));
}

#[test]
fn html_alignment_is_applied_in_every_mode() {
    let md = "<p align=\"center\">centred</p>\n\n<div align=\"right\">right</div>\n\n\
              <h2 style=\"text-align: center\">styled heading</h2>\n\n<center>old school</center>\n\n\
              <div align=\"left\">left</div>\n\nplain after\n";
    for cfg in [no_images(), md2docx::Config::untrusted()] {
        let (_, b) = html_docx(md, &cfg);
        assert_eq!(b.matches("w:jc w:val=\"center\"").count(), 3, "p, h2, center: {b}");
        assert_eq!(b.matches("w:jc w:val=\"right\"").count(), 1);
        assert!(b.contains("Heading2"));
        // Alignment ends with its element.
        let after = b.split("plain after").next().unwrap().rsplit("<w:p ").next().unwrap();
        assert!(
            !after.contains("w:jc"),
            "text after the block must not inherit alignment"
        );
    }
}

#[test]
fn html_lists_use_real_word_numbering() {
    let md = "<ol start=\"3\"><li>third</li><li>fourth<ul><li>nested a</li><li>nested b</li></ul></li></ol>\n\n<ul><li>bullet</li></ul>\n";
    let docx = markdown_to_docx(md, &no_images()).unwrap();
    let b = zip_text(&docx, "word/document.xml");
    assert_well_formed(&b);
    assert_eq!(b.matches("<w:numPr>").count(), 5);
    assert!(zip_text(&docx, "word/numbering.xml").contains("startOverride w:val=\"3\""));
    assert!(b.contains("w:ilvl w:val=\"1\""), "nested list is one level deeper");
    let text = visible_text(&b);
    assert!(text.contains("third") && text.contains("nested b") && !text.contains('\u{2022}'));
    // Stray list tags neither crash nor eat a Markdown list.
    let (_, b) = html_docx(
        "</ul></ol>\n\n- md one\n- md two\n\n</ul>\n\n<li>loose item</li>\n",
        &no_images(),
    );
    assert_eq!(b.matches("<w:numPr>").count(), 2);
    assert!(visible_text(&b).contains("loose item"));
}

#[test]
fn html_pre_keeps_whitespace_and_blockquote_is_a_quote() {
    let md = "<pre>\nfn main() {\n    let x = 1;\n\n    println!(\"{x}\");\n}\n</pre>\n\n<blockquote>quoted <b>html</b></blockquote>\n\nafter\n";
    let (_, b) = html_docx(md, &no_images());
    assert!(b.matches("CodeBlock").count() >= 5, "one code paragraph per line");
    let text = visible_text(&b);
    assert!(
        text.contains("    let x = 1;") && text.contains("fn main() {"),
        "indentation must survive: {text:?}"
    );
    assert!(b.contains("w:left w:val=\"single\""), "blockquote gets the quote bar");
    // A stray </blockquote> must not unbalance a Markdown quote.
    let (_, b) = html_docx("</blockquote>\n\n> md quote\n\ntext\n", &no_images());
    assert_eq!(b.matches("w:left w:val=\"single\"").count(), 1);
}

#[test]
fn html_tables_become_word_tables() {
    let md = "<table>\n<tr><th>Name</th><th>Qty</th></tr>\n<tr><td>apples</td><td>3</td></tr>\n<tr><td>pears</td><td><b>12</b></td></tr>\n</table>\n\nafter\n";
    for cfg in [no_images(), md2docx::Config::untrusted()] {
        let (_, b) = html_docx(md, &cfg);
        assert_eq!(b.matches("<w:tbl>").count(), 1);
        assert_eq!(b.matches("<w:tr>").count() + b.matches("<w:tr ").count(), 3);
        assert!(b.contains("E7E6E6"), "header cells are shaded");
        let text = visible_text(&b);
        for t in ["Name", "Qty", "apples", "3", "pears", "12", "after"] {
            assert!(text.contains(t), "{t}: {text}");
        }
    }
    // Sloppy HTML: missing </td>/</tr>/</table>, ragged rows, empty table, stray cells.
    for sloppy in [
        "<table><tr><td>a<td>b<tr><td>c</table>\n",
        "<table><tr><td>only one cell<tr><td>x<td>y<td>z\n",
        "<table></table>\n\ntext\n",
        "<td>stray</td><tr><th>head</th></tr>\n",
        "<table><tr><td><table><tr><td>nested</td></tr></table></td></tr></table>\n",
    ] {
        let (_, b) = html_docx(sloppy, &no_images());
        assert!(b.contains("<w:body>"), "{sloppy}");
    }
    let (_, b) = html_docx("<table></table>\n\ntext\n", &no_images());
    assert_eq!(
        b.matches("<w:tbl>").count(),
        0,
        "a table without rows would be invalid Word"
    );
    let (_, b) = html_docx("<table><tr><td>a<td>b<tr><td>c</table>\n", &no_images());
    assert_eq!(b.matches("<w:tr>").count() + b.matches("<w:tr ").count(), 2);
}

#[test]
fn html_img_width_sets_the_picture_size() {
    let fx = Fixture::new(); // fixture.png is 600x300 px
    let cfg = md2docx::Config::native_math().with_base_dir(fx.dir.path());
    let extent = |md: &str| -> (u64, u64) {
        let (_, b) = html_docx(md, &cfg);
        let tag = &b[b.find("<wp:extent").expect("a picture")..];
        let get = |k: &str| -> u64 {
            let at = tag.find(k).unwrap() + k.len();
            tag[at..].split('"').next().unwrap().parse().unwrap()
        };
        (get("cx=\""), get("cy=\""))
    };
    // 150 CSS px wide = 150 * 9525 EMU, height follows the 2:1 aspect ratio.
    assert_eq!(
        extent("<img src=\"fixture.png\" width=\"150\">"),
        (150 * 9525, 75 * 9525)
    );
    assert_eq!(
        extent("<img src=\"fixture.png\" width=\"150px\">"),
        (150 * 9525, 75 * 9525)
    );
    // 50% of the text width.
    let (cx, cy) = extent("<img src=\"fixture.png\" width=\"50%\">");
    assert!(cx > 2_000_000 && cx.abs_diff(2 * cy) <= 1, "{cx} x {cy}");
    // Nonsense widths fall back to the natural size (600 px at 96 dpi = 600 * 9525).
    for bad in ["abc", "0", "-5", "999999", "50%%", ""] {
        assert_eq!(
            extent(&format!("<img src=\"fixture.png\" width=\"{bad}\">")),
            (600 * 9525, 300 * 9525),
            "{bad}"
        );
    }
    // Wider than the page is clamped, never overflowing the margins.
    let (cx, _) = extent("<img src=\"fixture.png\" width=\"5000\">");
    assert!(cx <= 9026 * 635, "{cx}");
}

#[test]
fn code_blocks_keep_their_layout() {
    let b = body("```rust,ignore title=\"x\"\n    four spaces\n\ttab\ta\n\n  after blank  \nlast\n```\n");
    // The language and any attributes after it are not shown; the code is.
    let text = visible_text(&b);
    assert!(!text.contains("rust") && !text.contains("title"), "{text}");
    // Leading and trailing spaces survive.
    assert!(b.contains(">    four spaces<"), "{b}");
    assert!(b.contains(">  after blank  <"), "{b}");
    // Tabs are real tab elements, not raw characters inside the text.
    assert!(!b.contains('\t'), "raw tab character in document.xml");
    assert_eq!(b.matches("<w:tab/>").count(), 2, "{b}");
    // One shaded, monospace paragraph per line, including the blank one.
    assert_eq!(b.matches("w:val=\"CodeBlock\"").count(), 5, "{b}");
    assert_eq!(b.matches("w:fill=\"F2F2F2\"").count(), 5);
}

#[test]
fn code_blocks_inside_containers_are_indented() {
    let b = body("- item\n\n  ```\n  in list\n  ```\n\n> ```\n> in quote\n> ```\n");
    let text = visible_text(&b);
    assert!(text.contains("in list") && text.contains("in quote"), "{text}");
    assert_eq!(b.matches("w:val=\"CodeBlock\"").count(), 2);
    assert!(
        b.contains("w:left=\"720\""),
        "list code is indented under the item: {b}"
    );
    assert!(b.contains("w:left=\"360\""), "quoted code is indented: {b}");
}

#[test]
fn inline_code_spans() {
    let b = body("use `` a ` b `` and `x` and ` padded `\n");
    assert_eq!(b.matches("w:val=\"InlineCode\"").count(), 3, "{b}");
    let text = visible_text(&b);
    assert!(text.contains("a ` b") && text.contains("x"), "{text}");
}

#[test]
fn code_in_every_container_converts() {
    for (md, word) in [
        ("1. step\n\n   ```sh\n   run\n   ```\n", "run"),
        ("> quote\n>\n>     indented code\n", "indented code"),
        ("| a |\n|---|\n| `code` |\n", "code"),
        ("term\n: ```\n  code\n  ```\n", "code"),
        ("see[^n]\n\n[^n]: note\n\n    ```\n    in note\n    ```\n", "see"),
        (
            "<details>\n\n```\nfenced in html\n```\n\n</details>\n",
            "fenced in html",
        ),
        ("<pre>raw\n  pre</pre>\n", "raw"),
    ] {
        let b = body(md);
        assert!(visible_text(&b).contains(word), "`{word}` missing for {md:?}: {b}");
    }
}

#[test]
fn nested_image_alt_text_is_kept() {
    // Found by the CommonMark corpus: the outer alt text was lost.
    let cfg = Config::native_math().with_image_access(md2docx::ImageAccess::None);
    let b = body_with("![foo ![bar](/url)](/url2)\n", &cfg);
    assert!(visible_text(&b).contains("[foo bar]"), "{}", visible_text(&b));
}

#[test]
fn content_between_html_table_tags_is_not_lost() {
    // Found by the CommonMark corpus: an indented block (code, per CommonMark)
    // between table tags was silently dropped. It now appears in front of the table.
    let b = body("<table>\n\n  <tr>\n\n    <td>\n      Hi\n    </td>\n\n  </tr>\n\n</table>\n");
    let text = visible_text(&b);
    assert!(text.contains("<td>") && text.contains("Hi"), "{text}");
    // Markdown inside a real cell stays in the cell.
    let b = body("<table><tr><td>\n\n**bold** text\n\n</td></tr></table>\n");
    assert!(b.contains("<w:tbl>") && visible_text(&b).contains("bold"), "{b}");
}
