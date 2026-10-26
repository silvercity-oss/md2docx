//! The command line.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use silvercity_oss::{CommandSpec, PageSize};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    /// Editable Word equations (OMML), converted in-process. Runs no subprocess.
    Native,
    /// Transparent PNGs rendered with a local TeX install (`latex` + `dvipng`).
    Image,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MermaidEngine {
    /// `merman-cli`: headless Rust renderer, no browser (`cargo install merman-cli`).
    Merman,
    /// `mmdc`: the official Mermaid CLI; drives a headless Chrome.
    Mmdc,
    /// Rendered inside this process by the compiled-in `merman` library: no child process, no
    /// browser, nothing written to disk. Only in builds with the `mermaid` feature.
    #[cfg(feature = "mermaid")]
    Inprocess,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Plain grey code blocks (default).
    None,
    /// Syntax highlighting in the VS Code Light+ colours.
    Light,
    /// Syntax highlighting in the VS Code Dark+ colours.
    Dark,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeBlocks {
    /// One paragraph per source line (default).
    Lines,
    /// One paragraph per block, lines separated by line breaks: easy to select and copy.
    Single,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAccess {
    /// Never read local files; images become `[alt text]`.
    None,
    /// Only files inside the input file's directory (symlinks resolved).
    BaseDir,
    /// Any readable file. Only for Markdown you wrote yourself.
    Any,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Human-readable lines on standard error (default).
    Text,
    /// One JSON object per input on standard error, nothing else (see docs/cli.md).
    Json,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllowLinks {
    /// Relative and absolute paths stay text (default).
    None,
    /// Paths to existing files and directories inside the link root become hyperlinks; anything
    /// else (missing, outside the root, programs, network paths) stays text and is reported.
    Local,
    /// Every path becomes a hyperlink, existing or not, inside the root or not. Network paths are
    /// still refused. Only for documents you wrote.
    All,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStyle {
    /// Relative to where the .docx is saved: survives moving the whole tree.
    Rebased,
    /// A `file:///` address: survives moving the .docx on this machine.
    Absolute,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownLinks {
    /// A link to a Markdown file points at that file (default).
    Keep,
    /// A link to a Markdown file points at the .docx it becomes (and `#heading` lands on that
    /// heading). You convert those files too: list them, or use `follow`.
    Docx,
    /// Like `docx`, and every Markdown file reached through a link is converted as well.
    Follow,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkLayout {
    /// Keep the directory structure under the output directory (default).
    Mirror,
    /// Put every .docx directly in the output directory.
    Flat,
}

#[cfg(feature = "mermaid")]
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nfeatures: cli, mermaid (in-process Mermaid available)"
);
#[cfg(not(feature = "mermaid"))]
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\nfeatures: cli (in-process Mermaid not compiled in)"
);

#[derive(Parser, Debug)]
#[command(
    name = "md2docx",
    version,
    long_version = LONG_VERSION,
    about = "High-fidelity Markdown to DOCX compiler with native Math and Graph preprocessing",
    after_help = "Command templates may use {input} and {output} (and {dpi}); both of the first two are required.\n\
                  Example: --dolphin-command 'dolphin -i {input} -o {output}'\n\n\
                  EXIT STATUS: 0 converted | 1 other failure | 2 usage | 3 file I/O | 4 document rejected \
                  (malformed, over a limit, unsafe) | 5 --strict refusal | 6 external tool | 7 deadline. \
                  See docs/cli.md.\n\n\
                  SECURITY: `--math-mode image` runs a local TeX engine on document content, and diagram \
                  rendering runs external tools. Use `--untrusted` for input you do not control."
)]
pub struct Args {
    #[arg(
        value_name = "INPUT",
        required_unless_present_any = ["completions", "man"],
        help = "Markdown file(s) to convert; `-` reads standard input"
    )]
    pub inputs: Vec<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Output .docx file; a directory when converting several inputs; `-` writes to standard output [default: <input>.docx next to the input; standard output for `-`]"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        help = "Convert and report, but write nothing: for validating Markdown in CI. With --strict, findings are an error"
    )]
    pub check: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = Format::Text,
        env = "MD2DOCX_FORMAT",
        help = "How results are reported on standard error"
    )]
    pub format: Format,

    #[arg(
        short,
        long,
        env = "MD2DOCX_QUIET",
        value_parser = clap::builder::BoolishValueParser::new(),
        help = "Text format only: do not print warnings or the `wrote` line. Errors are always printed"
    )]
    pub quiet: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = MathMode::Native,
        help = "Math rendering strategy (Native OMML or Image fallback)"
    )]
    pub math_mode: MathMode,

    #[arg(
        long,
        value_parser = parse_command,
        help = "Custom CLI command layout for Dolphin graphs (e.g., 'dolphin -i {input} -o {output}')"
    )]
    pub dolphin_command: Option<CommandSpec>,

    #[arg(
        long,
        value_enum,
        default_value_t = MermaidEngine::Merman,
        help = "Renderer for ```mermaid blocks: merman (Rust, no browser), mmdc (official CLI, drives Chrome) or inprocess (compiled in, no child process; builds with the `mermaid` feature)"
    )]
    pub mermaid_engine: MermaidEngine,

    #[arg(
        long,
        value_parser = parse_command,
        conflicts_with = "mermaid_engine",
        help = "Custom CLI command layout for Mermaid graphs, replacing --mermaid-engine (e.g., 'mmdc -i {input} -o {output} -p puppeteer.json')"
    )]
    pub mermaid_command: Option<CommandSpec>,

    #[arg(
        long,
        env = "MD2DOCX_UNTRUSTED",
        value_parser = clap::builder::BoolishValueParser::new(),
        conflicts_with_all = ["math_mode", "dolphin_command", "mermaid_command", "mermaid_engine", "latex", "dvipng", "image_access"],
        help = "Strictest preset for untrusted Markdown: native math, diagram fences stay code, no local images, tight size limits. Starts no subprocess"
    )]
    pub untrusted: bool,

    #[arg(
        long,
        requires = "untrusted",
        help = "With --untrusted: also render Mermaid diagrams (in-process in builds with the `mermaid` feature, otherwise with merman-cli: no browser; `cargo install merman-cli`, one tightly bounded process per diagram)"
    )]
    pub diagrams: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = ImageAccess::BaseDir,
        help = "Which local images may be embedded"
    )]
    pub image_access: ImageAccess,

    #[arg(
        long,
        value_name = "DIR",
        env = "MD2DOCX_BASE_DIR",
        help = "Directory that relative image paths resolve against and --image-access base-dir confines to [default: the input file's directory; the current directory for `-`]"
    )]
    pub base_dir: Option<PathBuf>,

    #[arg(long, default_value = "latex", help = "`latex` executable (image math mode only)")]
    pub latex: String,

    #[arg(long, default_value = "dvipng", help = "`dvipng` executable (image math mode only)")]
    pub dvipng: String,

    #[arg(
        long,
        value_name = "SECONDS",
        env = "MD2DOCX_TIMEOUT",
        help = "Kill an external renderer after this long [default: 60; 20 with --untrusted --diagrams]"
    )]
    pub timeout: Option<u64>,

    #[arg(
        long,
        value_name = "0-9",
        default_value_t = 6,
        value_parser = clap::value_parser!(u32).range(0..=9),
        help = "Compression of the .docx: 0 stores it uncompressed, 1 is fastest, 9 smallest"
    )]
    pub compression_level: u32,

    #[arg(
        long,
        env = "MD2DOCX_STRICT",
        value_parser = clap::builder::BoolishValueParser::new(),
        help = "Fail instead of writing a document that could not be converted faithfully (a dropped image or link, a diagram left as code, an unknown LaTeX command, ...). Every problem is listed with its line and column"
    )]
    pub strict: bool,

    #[arg(
        long,
        value_name = "FILE.docx",
        help = "Take the styles and theme (fonts, sizes, colours, heading looks) from this .docx; styles it lacks are added"
    )]
    pub reference_doc: Option<PathBuf>,

    #[arg(long, help = "Number headings 1, 1.1, 1.1.1 ...")]
    pub number_headings: bool,

    #[arg(
        long,
        value_name = "MAX_LEVEL",
        num_args = 0..=1,
        default_missing_value = "3",
        value_parser = clap::value_parser!(u8).range(1..=6),
        help = "Start the document with a linked table of contents down to this heading level [default: 3]"
    )]
    pub toc: Option<u8>,

    #[arg(long, help = "Turn bare http://, https:// and www. addresses into links")]
    pub autolink: bool,

    #[arg(long, help = "Read ^x^ as superscript and ~x~ as subscript")]
    pub sup_sub: bool,

    #[arg(
        long,
        value_name = "LIST",
        value_delimiter = ',',
        help = "URL schemes a link may use, comma-separated, replacing the default (http,https,mailto). With --untrusted the list may only be shorter"
    )]
    pub link_schemes: Option<Vec<String>>,

    #[arg(
        long,
        help = "Do not turn raw HTML <a href> links into hyperlinks (they are off with --untrusted)"
    )]
    pub no_html_links: bool,

    #[arg(
        long,
        value_name = "TEXT",
        help = "Document title in the file's properties [default: `title:` front matter]"
    )]
    pub title: Option<String>,

    #[arg(
        long,
        value_name = "TEXT",
        help = "Document author in the file's properties [default: `author:` front matter]"
    )]
    pub author: Option<String>,

    #[arg(
        long,
        value_name = "SIZE",
        value_parser = parse_page_size,
        help = "Page size: a4 (default), letter, legal, or WIDTHxHEIGHT with a unit, e.g. 148x210mm or 8x10in"
    )]
    pub page_size: Option<PageSize>,

    #[arg(long, requires = "page_size", help = "Swap the page's width and height")]
    pub landscape: bool,

    #[arg(
        long,
        value_name = "LENGTH",
        value_parser = parse_length,
        help = "Page margin on every side, with a unit: 25mm, 2.5cm, 1in, 72pt [default: 1in]"
    )]
    pub margin: Option<u32>,

    #[arg(long, value_name = "NAME", help = "Body font [default: Calibri]")]
    pub body_font: Option<String>,

    #[arg(
        long,
        value_name = "POINTS",
        value_parser = parse_font_size,
        help = "Body text size in points, in steps of 0.5 [default: 11]"
    )]
    pub body_size: Option<u32>,

    #[arg(long, value_name = "NAME", help = "Code font [default: Consolas]")]
    pub code_font: Option<String>,

    #[arg(long, help = "Show the page number, centred in the footer")]
    pub page_numbers: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = AllowLinks::None,
        conflicts_with_all = ["untrusted"],
        help = "Turn links to local files and directories into hyperlinks: `local` for existing ones inside the link root, `all` for any path. Off by default: a relative link is shown as text. See docs/links.md"
    )]
    pub allow_links: AllowLinks,

    #[arg(
        long,
        value_name = "DIR",
        help = "The tree local links must stay inside [default: the input's directory, or the common directory of the inputs with --markdown-links docx|follow]"
    )]
    pub link_root: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        default_value_t = LinkStyle::Rebased,
        help = "Write local links relative to where the .docx is saved, or as file:/// addresses"
    )]
    pub link_style: LinkStyle,

    #[arg(
        long,
        value_name = "URL",
        help = "Make local files into web links instead: docs/a.md becomes URL + docs/a.md, e.g. https://github.com/me/repo/blob/v1.0/"
    )]
    pub link_base: Option<String>,

    #[arg(
        long,
        value_name = "URL",
        help = "Like --link-base, for directories (…/tree/v1.0/) [default: --link-base]"
    )]
    pub link_tree_base: Option<String>,

    #[arg(
        long,
        value_name = "PREFIX=URL",
        value_parser = parse_link_map,
        help = "Web address for local paths under PREFIX (relative to the link root, `/`-separated); repeatable, first match wins"
    )]
    pub link_map: Vec<(String, String)>,

    #[arg(long, help = "Do not check that `file.md#heading` names a heading that exists")]
    pub no_check_link_anchors: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = MarkdownLinks::Keep,
        help = "What a link to another Markdown file points at: that file (keep), the .docx it becomes (docx), or also convert it (follow)"
    )]
    pub markdown_links: MarkdownLinks,

    #[arg(
        long,
        value_enum,
        default_value_t = LinkLayout::Mirror,
        help = "With --markdown-links docx|follow: where the .docx files go under -o DIR"
    )]
    pub link_layout: LinkLayout,

    #[arg(
        long,
        help = "With --markdown-links docx|follow and -o DIR: copy the other files and directories the documents link to into DIR (same relative path), so the links work in the output tree"
    )]
    pub copy_linked: bool,

    #[arg(
        long,
        value_name = "COUNT",
        default_value_t = 1000,
        help = "Most documents converted plus files copied through links in one run"
    )]
    pub max_linked_files: usize,

    #[arg(
        long,
        value_name = "BYTES",
        value_parser = parse_size,
        default_value = "256M",
        help = "Most bytes --copy-linked may copy in one run"
    )]
    pub max_copy_bytes: u64,

    #[arg(
        long,
        value_name = "SECONDS",
        env = "SOURCE_DATE_EPOCH",
        help = "Set the document's created/modified time (Unix seconds) so the same input gives the same bytes. Honours the SOURCE_DATE_EPOCH convention"
    )]
    pub source_date_epoch: Option<u64>,

    #[arg(
        long,
        value_name = "DIR",
        env = "MD2DOCX_SCRATCH_DIR",
        help = "Create the temporary working directory under DIR instead of the system temp directory (DIR must exist)"
    )]
    pub scratch_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "SECONDS",
        env = "MD2DOCX_DEADLINE",
        help = "Stop the whole conversion after this long [default: none; 120 with --untrusted]"
    )]
    pub deadline: Option<u64>,

    #[arg(
        long,
        value_name = "BYTES",
        value_parser = parse_size,
        env = "MD2DOCX_MAX_INPUT_BYTES",
        help = "Largest Markdown input, e.g. 2M [default: 10M; 2M with --untrusted]"
    )]
    pub max_input_bytes: Option<u64>,

    #[arg(
        long,
        value_name = "BYTES",
        value_parser = parse_size,
        env = "MD2DOCX_MAX_OUTPUT_BYTES",
        help = "Largest .docx produced [default: 1G; 50M with --untrusted]"
    )]
    pub max_output_bytes: Option<u64>,

    #[arg(
        long,
        value_name = "BYTES",
        value_parser = parse_size,
        env = "MD2DOCX_MAX_UNCOMPRESSED_BYTES",
        help = "Largest total size of the document's parts before compression [default: 1G; 64M with --untrusted]"
    )]
    pub max_uncompressed_bytes: Option<u64>,

    #[arg(
        long,
        value_name = "COUNT",
        env = "MD2DOCX_MAX_ELEMENTS",
        help = "Most paragraphs, runs, cells and list instances; bounds memory while building [default: 1000000; 60000 with --untrusted]"
    )]
    pub max_elements: Option<usize>,

    #[arg(
        long,
        value_name = "BYTES",
        value_parser = parse_size,
        env = "MD2DOCX_MAX_IMAGE_BYTES",
        help = "Largest single image file or rendered diagram"
    )]
    pub max_image_bytes: Option<u64>,

    #[arg(
        long,
        value_name = "COUNT",
        env = "MD2DOCX_MAX_IMAGES",
        help = "Most distinct images in the document"
    )]
    pub max_images: Option<usize>,

    #[arg(
        long,
        help = "If a mermaid/dolphin block cannot be rendered (tool missing, invalid diagram), keep it as a code block instead of failing"
    )]
    pub keep_failed_diagrams: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = CodeBlocks::Lines,
        help = "Code block layout: one paragraph per line, or the whole block as one paragraph (triple-click selects it)"
    )]
    pub code_blocks: CodeBlocks,

    #[arg(
        long,
        value_enum,
        default_value_t = Highlight::None,
        help = "Syntax-highlight code blocks with the built-in lexers (Rust, Python, JS/TS, Java, C/C++, C#, Go, Kotlin, Swift, Ruby, PHP, SQL, Bash, JSON, YAML, TOML, HTML/XML, CSS, diff)"
    )]
    pub highlight: Highlight,

    #[arg(
        long,
        value_name = "SHELL",
        value_enum,
        help = "Print a completion script for SHELL and exit"
    )]
    pub completions: Option<clap_complete::Shell>,

    #[arg(long, help = "Print the manual page (roff) and exit")]
    pub man: bool,
}

/// Split a command template with POSIX shell rules (via `shlex`) and check it
/// can actually produce an image.
pub fn parse_command(template: &str) -> Result<CommandSpec, String> {
    let mut words = shlex::split(template)
        .ok_or("unbalanced quotes in command")?
        .into_iter();
    let program = words.next().ok_or("command is empty")?;
    let args: Vec<String> = words.collect();
    let mentions = |p: &str| program.contains(p) || args.iter().any(|a| a.contains(p));
    if !mentions("{input}") || !mentions("{output}") {
        return Err("command must contain both {input} and {output} placeholders".into());
    }
    Ok(CommandSpec::new(program, args))
}

/// `PREFIX=URL`.
pub fn parse_link_map(text: &str) -> Result<(String, String), String> {
    let (prefix, url) = text
        .split_once('=')
        .ok_or_else(|| format!("`{text}` is not PREFIX=URL"))?;
    if url.is_empty() {
        return Err(format!("`{text}` has no URL"));
    }
    Ok((prefix.to_string(), url.to_string()))
}

/// A byte count: digits with an optional binary suffix (`K`, `M`, `G`, optionally `iB`/`B`).
/// `2M` is 2 * 1024 * 1024.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let t = text.trim();
    let digits = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (number, suffix) = t.split_at(digits);
    let n: u64 = number
        .parse()
        .map_err(|_| format!("`{text}` is not a size (expected digits and an optional K, M or G)"))?;
    let shift = match suffix.to_ascii_lowercase().as_str() {
        "" | "b" => 0,
        "k" | "kb" | "kib" => 10,
        "m" | "mb" | "mib" => 20,
        "g" | "gb" | "gib" => 30,
        _ => return Err(format!("`{text}`: unknown size suffix `{suffix}` (use K, M or G)")),
    };
    n.checked_mul(1u64 << shift)
        .ok_or_else(|| format!("`{text}` is too large"))
}

/// A length in twentieths of a point, from a number and a unit: `mm`, `cm`, `in`, `pt`.
pub fn parse_length(text: &str) -> Result<u32, String> {
    let t = text.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .ok_or_else(|| format!("`{text}` needs a unit: mm, cm, in or pt"))?;
    let (number, unit) = t.split_at(split);
    let value: f64 = number.parse().map_err(|_| format!("`{text}` is not a length"))?;
    let per_unit = match unit.to_ascii_lowercase().as_str() {
        "mm" => 1440.0 / 25.4,
        "cm" => 1440.0 / 2.54,
        "in" => 1440.0,
        "pt" => 20.0,
        _ => return Err(format!("`{text}`: unknown unit `{unit}` (use mm, cm, in or pt)")),
    };
    let twips = (value * per_unit).round();
    if !(0.0..=1_000_000.0).contains(&twips) {
        return Err(format!("`{text}` is out of range"));
    }
    Ok(twips as u32)
}

/// A named page size, or `WIDTHxHEIGHT` with a unit.
pub fn parse_page_size(text: &str) -> Result<PageSize, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "a4" => return Ok(PageSize::A4),
        "letter" => return Ok(PageSize::Letter),
        "legal" => return Ok(PageSize::Legal),
        _ => {}
    }
    let lower = text.trim().to_ascii_lowercase();
    let (w, h) = lower
        .split_once('x')
        .ok_or_else(|| format!("`{text}` is not a page size (a4, letter, legal, or e.g. 148x210mm)"))?;
    // The unit may follow either number: `8x10in` and `8inx10in` both work.
    let unit_of = |s: &str| {
        s.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
            .to_string()
    };
    let unit = if unit_of(h).is_empty() { unit_of(w) } else { unit_of(h) };
    let with_unit = |s: &str| {
        if unit_of(s).is_empty() {
            format!("{s}{unit}")
        } else {
            s.to_string()
        }
    };
    Ok(PageSize::Custom {
        width_twips: parse_length(&with_unit(w))?,
        height_twips: parse_length(&with_unit(h))?,
    })
}

/// Points, in steps of 0.5, as the half-points Word stores.
pub fn parse_font_size(text: &str) -> Result<u32, String> {
    let points: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a font size in points"))?;
    let half = (points * 2.0).round();
    if !(2.0..=3276.0).contains(&half) {
        return Err(format!("`{text}` is out of range (1 to 1638 points)"));
    }
    Ok(half as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("2M").unwrap(), 2 << 20);
        assert_eq!(parse_size("2MiB").unwrap(), 2 << 20);
        assert_eq!(parse_size("64k").unwrap(), 64 << 10);
        assert_eq!(parse_size("1G").unwrap(), 1 << 30);
        for bad in [
            "",
            "M",
            "1.5M",
            "1X",
            "-1",
            "99999999999999999999",
            "18446744073709551615G",
        ] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lengths_and_page_sizes() {
        assert_eq!(parse_length("1in").unwrap(), 1440);
        assert_eq!(parse_length("72pt").unwrap(), 1440);
        assert_eq!(parse_length("25.4mm").unwrap(), 1440);
        assert_eq!(parse_length("2.54cm").unwrap(), 1440);
        assert!(parse_length("1").is_err());
        assert!(parse_length("1furlong").is_err());
        assert!(parse_length("-1in").is_err());
        assert_eq!(parse_page_size("A4").unwrap(), PageSize::A4);
        assert_eq!(parse_page_size("letter").unwrap(), PageSize::Letter);
        assert_eq!(
            parse_page_size("8x10in").unwrap(),
            PageSize::Custom {
                width_twips: 11520,
                height_twips: 14400
            }
        );
        assert_eq!(parse_page_size("8inx10in").unwrap(), parse_page_size("8x10in").unwrap());
        assert!(parse_page_size("a5").is_err());
        assert!(parse_page_size("8x10").is_err());
    }

    #[test]
    fn font_sizes() {
        assert_eq!(parse_font_size("11").unwrap(), 22);
        assert_eq!(parse_font_size("10.5").unwrap(), 21);
        assert!(parse_font_size("0").is_err());
        assert!(parse_font_size("x").is_err());
    }

    #[test]
    fn command_templates_are_split_like_a_shell() {
        let c = parse_command(r#"dolphin --title "my graph" -i {input} -o {output}"#).unwrap();
        assert_eq!(c.program, "dolphin");
        assert_eq!(c.args, ["--title", "my graph", "-i", "{input}", "-o", "{output}"]);
    }

    #[test]
    fn bad_command_templates_are_rejected() {
        assert!(parse_command("").is_err());
        assert!(parse_command("dolphin 'unterminated -i {input} -o {output}").is_err());
        assert!(parse_command("dolphin -i {input}").is_err());
        assert!(parse_command("dolphin -o {output}").is_err());
    }

    #[test]
    fn clap_definition_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
