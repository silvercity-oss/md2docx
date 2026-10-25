//! Every construct pulldown-cmark can report is either rendered or deliberately
//! left off, and the test fails if that stops being true.
//!
//! `tag_name` and `event_name` match *exhaustively* (no wildcard), so a
//! pulldown-cmark upgrade that adds a variant stops compiling here until someone
//! decides how this crate should treat it and adds a fixture below.
//!
//! The parser options are repeated from `parser.rs`; the fixtures check that
//! they still produce what each fixture claims to exercise.

mod common;

use std::collections::BTreeSet;

use common::*;
use pulldown_cmark::{Event, Options, Parser, Tag};
use silvercity_oss::{markdown_to_docx, Config, ImageAccess};

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_MATH
}

fn tag_name(t: &Tag<'_>) -> &'static str {
    match t {
        Tag::Paragraph => "Paragraph",
        Tag::Heading { .. } => "Heading",
        Tag::BlockQuote(_) => "BlockQuote",
        Tag::CodeBlock(_) => "CodeBlock",
        Tag::HtmlBlock => "HtmlBlock",
        Tag::List(_) => "List",
        Tag::Item => "Item",
        Tag::FootnoteDefinition(_) => "FootnoteDefinition",
        Tag::DefinitionList => "DefinitionList",
        Tag::DefinitionListTitle => "DefinitionListTitle",
        Tag::DefinitionListDefinition => "DefinitionListDefinition",
        Tag::Table(_) => "Table",
        Tag::TableHead => "TableHead",
        Tag::TableRow => "TableRow",
        Tag::TableCell => "TableCell",
        Tag::Emphasis => "Emphasis",
        Tag::Strong => "Strong",
        Tag::Strikethrough => "Strikethrough",
        Tag::Superscript => "Superscript",
        Tag::Subscript => "Subscript",
        Tag::Link { .. } => "Link",
        Tag::Image { .. } => "Image",
        Tag::MetadataBlock(_) => "MetadataBlock",
    }
}

fn event_name(e: &Event<'_>) -> &'static str {
    match e {
        Event::Start(_) => "Start",
        Event::End(_) => "End",
        Event::Text(_) => "Text",
        Event::Code(_) => "Code",
        Event::InlineMath(_) => "InlineMath",
        Event::DisplayMath(_) => "DisplayMath",
        Event::Html(_) => "Html",
        Event::InlineHtml(_) => "InlineHtml",
        Event::FootnoteReference(_) => "FootnoteReference",
        Event::SoftBreak => "SoftBreak",
        Event::HardBreak => "HardBreak",
        Event::Rule => "Rule",
        Event::TaskListMarker(_) => "TaskListMarker",
    }
}

/// Constructs this crate renders.
const RENDERED_TAGS: &[&str] = &[
    "Paragraph",
    "Heading",
    "BlockQuote",
    "CodeBlock",
    "HtmlBlock",
    "List",
    "Item",
    "FootnoteDefinition",
    "DefinitionList",
    "DefinitionListTitle",
    "DefinitionListDefinition",
    "Table",
    "TableHead",
    "TableRow",
    "TableCell",
    "Emphasis",
    "Strong",
    "Strikethrough",
    "Link",
    "Image",
];

/// Constructs whose parser extension is deliberately not enabled (the syntax
/// stays literal text; see the README).
const DISABLED_TAGS: &[&str] = &["Superscript", "Subscript", "MetadataBlock"];

const RENDERED_EVENTS: &[&str] = &[
    "Start",
    "End",
    "Text",
    "Code",
    "InlineMath",
    "DisplayMath",
    "Html",
    "InlineHtml",
    "FootnoteReference",
    "SoftBreak",
    "HardBreak",
    "Rule",
    "TaskListMarker",
];

/// What a fixture must produce: text visible in the document (or its footnotes),
/// and fragments of `document.xml`.
struct Fixture {
    md: &'static str,
    text: &'static [&'static str],
    xml: &'static [&'static str],
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        md: "para-alpha\n",
        text: &["para-alpha"],
        xml: &["<w:p "],
    },
    Fixture {
        md: "# head-alpha\n\n###### head-beta\n",
        text: &["head-alpha", "head-beta"],
        xml: &["Heading1", "Heading6"],
    },
    Fixture {
        md: "> quote-alpha\n",
        text: &["quote-alpha"],
        xml: &["<w:ind "],
    },
    Fixture {
        md: "> [!NOTE]\n> alert-alpha\n",
        text: &["alert-alpha"],
        xml: &[],
    },
    Fixture {
        md: "```rust\ncode-alpha\n```\n",
        text: &["code-alpha"],
        xml: &["CodeBlock"],
    },
    Fixture {
        md: "<div>html-alpha</div>\n\n<p>html-beta</p>\n",
        text: &["html-alpha", "html-beta"],
        xml: &[],
    },
    Fixture {
        md: "- item-alpha\n- item-beta\n  - item-gamma\n",
        text: &["item-alpha", "item-beta", "item-gamma"],
        xml: &["<w:numPr>"],
    },
    Fixture {
        md: "3. item-delta\n4. item-epsilon\n",
        text: &["item-delta", "item-epsilon"],
        xml: &["<w:numPr>"],
    },
    Fixture {
        md: "- [x] task-alpha\n- [ ] task-beta\n",
        text: &["task-alpha", "task-beta"],
        xml: &[],
    },
    Fixture {
        md: "Ref[^1].\n\n[^1]: foot-alpha\n",
        text: &["Ref", "foot-alpha"],
        xml: &["<w:footnoteReference"],
    },
    Fixture {
        md: "term-alpha\n: def-alpha\n",
        text: &["term-alpha", "def-alpha"],
        xml: &[],
    },
    Fixture {
        md: "| h-alpha | h-beta |\n|:--|--:|\n| c-alpha | c-beta |\n",
        text: &["h-alpha", "h-beta", "c-alpha", "c-beta"],
        xml: &["<w:tbl>", "<w:tr>", "<w:tc>"],
    },
    Fixture {
        md: "*em-alpha* _em-beta_\n",
        text: &["em-alpha", "em-beta"],
        xml: &["<w:i/>"],
    },
    Fixture {
        md: "**strong-alpha** __strong-beta__\n",
        text: &["strong-alpha", "strong-beta"],
        xml: &["<w:b/>"],
    },
    Fixture {
        md: "~~strike-alpha~~\n",
        text: &["strike-alpha"],
        xml: &["<w:strike/>"],
    },
    Fixture {
        md: "[link-alpha](https://example.com/a)\n",
        text: &["link-alpha"],
        xml: &["<w:hyperlink"],
    },
    Fixture {
        md: "![img-alpha](missing.png)\n",
        text: &["img-alpha"],
        xml: &[],
    },
    Fixture {
        md: "code `code-alpha` end\n",
        text: &["code-alpha"],
        xml: &["InlineCode"],
    },
    Fixture {
        md: "inline $x^2$ math\n",
        text: &["inline", "math"],
        xml: &["<m:oMath>"],
    },
    Fixture {
        md: "$$\\frac{a}{b}$$\n",
        text: &[],
        xml: &["<m:oMathPara>"],
    },
    Fixture {
        md: "soft-one\nsoft-two\n",
        text: &["soft-one soft-two"],
        xml: &[],
    },
    Fixture {
        md: "hard-one\\\nhard-two\n",
        text: &["hard-one", "hard-two"],
        xml: &["<w:br"],
    },
    Fixture {
        md: "a <b>inline-alpha</b> b\n",
        text: &["inline-alpha"],
        xml: &["<w:b/>"],
    },
    Fixture {
        md: "rule-alpha\n\n---\n\nrule-beta\n",
        text: &["rule-alpha", "rule-beta"],
        xml: &[],
    },
];

/// Syntax of extensions that are off: it must come through as written.
const LITERAL: &[&str] = &["x^sup^", "H~sub~O", "[[wiki-link]]", "\"quoted\" -- dash..."];

fn config() -> Config {
    Config::native_math().with_image_access(ImageAccess::None)
}

#[test]
fn every_parser_construct_is_accounted_for() {
    let mut tags = BTreeSet::new();
    let mut events = BTreeSet::new();
    for f in FIXTURES {
        for ev in Parser::new_ext(f.md, options()) {
            events.insert(event_name(&ev));
            if let Event::Start(t) = &ev {
                tags.insert(tag_name(t));
            }
        }
    }
    let rendered: BTreeSet<_> = RENDERED_TAGS.iter().copied().collect();
    let disabled: BTreeSet<_> = DISABLED_TAGS.iter().copied().collect();
    assert_eq!(
        tags.difference(&rendered).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "fixtures produce tags that are not listed as rendered"
    );
    assert_eq!(
        rendered.difference(&tags).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "tags listed as rendered have no fixture"
    );
    assert!(
        tags.is_disjoint(&disabled),
        "an extension listed as disabled is being produced: {:?}",
        tags.intersection(&disabled).collect::<Vec<_>>()
    );
    let want: BTreeSet<_> = RENDERED_EVENTS.iter().copied().collect();
    assert_eq!(events, want, "events seen across the fixtures");
}

#[test]
fn every_rendered_construct_reaches_the_document() {
    let cfg = config();
    for f in FIXTURES {
        let docx = markdown_to_docx(f.md, &cfg).unwrap_or_else(|e| panic!("{:?}: {e}", f.md));
        let doc = zip_text(&docx, "word/document.xml");
        assert_well_formed(&doc);
        let mut text = visible_text(&doc);
        if zip_entries(&docx).iter().any(|n| n == "word/footnotes.xml") {
            text.push_str(&visible_text(&zip_text(&docx, "word/footnotes.xml")));
        }
        for t in f.text {
            assert!(text.contains(t), "{:?}: text lacks `{t}`; got `{text}`", f.md);
        }
        for x in f.xml {
            assert!(doc.contains(x), "{:?}: document.xml lacks `{x}`\n{doc}", f.md);
        }
    }
}

#[test]
fn extensions_that_are_off_stay_literal() {
    let md = LITERAL.join("\n\n");
    let mut seen = BTreeSet::new();
    for ev in Parser::new_ext(&md, options()) {
        if let Event::Start(t) = &ev {
            seen.insert(tag_name(t));
        }
    }
    for t in DISABLED_TAGS {
        assert!(!seen.contains(t), "{t} is produced although its extension is off");
    }
    let text = visible_text(&body(&md));
    for lit in LITERAL {
        assert!(text.contains(lit), "`{lit}` was altered; got `{text}`");
    }
}
