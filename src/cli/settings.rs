//! From the command line to a library [`Config`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use silvercity_oss::{
    CodeBlockLayout, Config, DiagramFailure, Highlighting, ImageAccess as LibImageAccess, LossPolicy, MathImageEngine,
    MathMode as LibMathMode, MermaidEngine as LibMermaidEngine, PageSize, TableOfContents, Theme,
};

use super::args::{Args, CodeBlocks, Highlight, ImageAccess, MathMode, MermaidEngine};
use super::exit::CliError;

/// Longest per-tool timeout `--untrusted --diagrams` accepts (the preset's own).
pub const UNTRUSTED_MAX_TIMEOUT_SECS: u64 = 20;

/// A limit that `--untrusted` may lower but not raise: the preset is the policy for hostile
/// input, so a flag (or an environment variable a container sets) cannot quietly weaken it.
fn lower_only<T: PartialOrd + std::fmt::Display + Copy>(
    untrusted: bool,
    flag: &str,
    requested: Option<T>,
    current: &mut T,
) -> Result<(), CliError> {
    if let Some(v) = requested {
        if untrusted && v > *current {
            return Err(CliError::usage(format!(
                "{flag} {v} is above the --untrusted limit of {current}; --untrusted limits can only be lowered"
            )));
        }
        *current = v;
    }
    Ok(())
}

pub fn config(args: &Args) -> Result<Config, CliError> {
    let mut cfg = if args.untrusted {
        let cfg = if args.diagrams {
            diagrams_untrusted()
        } else {
            Config::untrusted()
        };
        if let Some(t) = args.timeout.filter(|_| args.diagrams) {
            if t > UNTRUSTED_MAX_TIMEOUT_SECS {
                return Err(CliError::usage(format!(
                    "--timeout may not exceed {UNTRUSTED_MAX_TIMEOUT_SECS} seconds with --untrusted --diagrams"
                )));
            }
        }
        cfg
    } else {
        let mut cfg = Config::trusted();
        cfg.math.mode = match args.math_mode {
            MathMode::Native => LibMathMode::NativeOmml,
            MathMode::Image => LibMathMode::ImageFallback,
        };
        cfg.math.engine = MathImageEngine::LatexDvipng {
            latex: args.latex.clone(),
            dvipng: args.dvipng.clone(),
        };
        cfg.image_access = match args.image_access {
            ImageAccess::None => LibImageAccess::None,
            ImageAccess::BaseDir => LibImageAccess::BaseDir,
            ImageAccess::Any => LibImageAccess::Any,
        };
        match args.mermaid_engine {
            MermaidEngine::Merman => cfg.graphs.use_engine(LibMermaidEngine::Merman),
            MermaidEngine::Mmdc => cfg.graphs.use_engine(LibMermaidEngine::Mmdc),
            #[cfg(feature = "mermaid")]
            MermaidEngine::Inprocess => cfg = cfg.with_in_process_mermaid(),
        }
        if let Some(spec) = &args.mermaid_command {
            cfg.graphs.mermaid = spec.clone();
        }
        if let Some(spec) = &args.dolphin_command {
            cfg.graphs.dolphin = spec.clone();
        }
        cfg
    };

    if args.no_html_links {
        cfg.html_links = false;
    }
    if let Some(list) = &args.link_schemes {
        let schemes: Vec<String> = list
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if args.untrusted {
            if let Some(extra) = schemes
                .iter()
                .find(|s| !cfg.allowed_link_schemes.iter().any(|a| a.eq_ignore_ascii_case(s)))
            {
                return Err(CliError::usage(format!(
                    "--link-schemes: `{extra}` is not allowed with --untrusted (the list may only be shorter than {})",
                    cfg.allowed_link_schemes.join(",")
                )));
            }
        }
        cfg.allowed_link_schemes = schemes;
    }

    // Limits: `--untrusted` can only be tightened.
    let u = args.untrusted;
    let limits = &mut cfg.limits;
    lower_only(
        u,
        "--max-input-bytes",
        args.max_input_bytes.map(|v| usize::try_from(v).unwrap_or(usize::MAX)),
        &mut limits.max_input_bytes,
    )?;
    lower_only(
        u,
        "--max-output-bytes",
        args.max_output_bytes,
        &mut limits.max_output_bytes,
    )?;
    lower_only(
        u,
        "--max-uncompressed-bytes",
        args.max_uncompressed_bytes,
        &mut limits.max_uncompressed_bytes,
    )?;
    lower_only(u, "--max-elements", args.max_elements, &mut limits.max_elements)?;
    lower_only(
        u,
        "--max-image-bytes",
        args.max_image_bytes,
        &mut limits.max_image_bytes,
    )?;
    lower_only(u, "--max-images", args.max_images, &mut limits.max_distinct_images)?;
    if let Some(seconds) = args.deadline {
        let requested = Duration::from_secs(seconds);
        if let (true, Some(preset)) = (u, limits.max_conversion_time) {
            if requested > preset {
                return Err(CliError::usage(format!(
                    "--deadline {seconds} is above the --untrusted limit of {} seconds; --untrusted limits can only be lowered",
                    preset.as_secs()
                )));
            }
        }
        limits.max_conversion_time = Some(requested);
    }

    cfg.compression_level = args.compression_level;
    cfg.autolink_bare_urls = args.autolink;
    cfg.superscript_subscript = args.sup_sub;
    let doc = &mut cfg.document;
    doc.number_headings = args.number_headings;
    doc.page_numbers = args.page_numbers;
    if let Some(level) = args.toc {
        doc.table_of_contents = Some(TableOfContents::new().max_level(level));
    }
    if let Some(t) = &args.title {
        doc.title = Some(t.clone());
    }
    if let Some(a) = &args.author {
        doc.author = Some(a.clone());
    }
    if let Some(page) = args.page_size {
        let (w, h) = page.twips();
        doc.page = if args.landscape {
            PageSize::Custom {
                width_twips: h,
                height_twips: w,
            }
        } else {
            page
        };
    }
    if let Some(m) = args.margin {
        doc.margin_twips = m;
    }
    if let Some(f) = &args.body_font {
        doc.body_font = f.clone();
    }
    if let Some(s) = args.body_size {
        doc.body_size_half_points = s;
    }
    if let Some(f) = &args.code_font {
        doc.code_font = f.clone();
    }
    doc.code_blocks = match args.code_blocks {
        CodeBlocks::Lines => CodeBlockLayout::LinePerParagraph,
        CodeBlocks::Single => CodeBlockLayout::SingleParagraph,
    };
    if let Some(secs) = args.source_date_epoch {
        doc.timestamp = Some(
            UNIX_EPOCH
                .checked_add(Duration::from_secs(secs))
                .unwrap_or(SystemTime::UNIX_EPOCH),
        );
    }
    if let Some(path) = &args.reference_doc {
        let bytes = std::fs::read(path)
            .map_err(|e| CliError::usage(format!("cannot read --reference-doc {}: {e}", path.display())))?;
        cfg = cfg
            .with_reference_docx(&bytes)
            .map_err(|e| CliError::usage(format!("--reference-doc {}: {e}", path.display())))?;
    }
    if args.strict {
        cfg.loss_policy = LossPolicy::Reject;
    }
    if let Some(dir) = &args.scratch_dir {
        cfg.scratch_dir = Some(dir.clone());
    }
    if let Some(dir) = &args.base_dir {
        cfg.base_dir = Some(dir.clone());
    }
    if args.keep_failed_diagrams {
        cfg.graphs.on_failure = DiagramFailure::KeepAsCode;
    }
    cfg.highlighting = match args.highlight {
        Highlight::None => None,
        Highlight::Light => Some(Highlighting::new(Theme::vscode_light())),
        Highlight::Dark => Some(Highlighting::new(Theme::vscode_dark())),
    };
    if let Some(seconds) = args.timeout {
        cfg.tool_timeout = Duration::from_secs(seconds);
    }
    cfg.validate().map_err(CliError::from)?;
    Ok(cfg)
}

/// `--untrusted --diagrams`: with the `mermaid` feature the diagrams are rendered inside this
/// process (no child process at all), otherwise by a tightly bounded `merman-cli`.
fn diagrams_untrusted() -> Config {
    #[cfg(feature = "mermaid")]
    {
        Config::untrusted().with_in_process_mermaid()
    }
    #[cfg(not(feature = "mermaid"))]
    {
        Config::untrusted_with_diagrams()
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use silvercity_oss::{ImageAccess as LibImageAccess, MathMode as LibMathMode};

    use super::*;

    fn parse(extra: &[&str]) -> Args {
        let mut v = vec!["md2docx", "in.md"];
        v.extend_from_slice(extra);
        Args::try_parse_from(v).unwrap()
    }

    #[test]
    fn untrusted_preset_is_strict_and_exclusive() {
        let a = parse(&["--untrusted"]);
        let c = config(&a).unwrap();
        assert_eq!(c.math.mode, LibMathMode::NativeOmml);
        assert!(!c.graphs.enabled);
        assert_eq!(c.image_access, LibImageAccess::None);
        // It cannot be combined with switches that would loosen it.
        for extra in [
            ["--math-mode", "image"],
            ["--image-access", "any"],
            ["--dolphin-command", "d -i {input} -o {output}"],
            ["--latex", "xelatex"],
        ] {
            assert!(
                Args::try_parse_from(["md2docx", "in.md", "--untrusted", extra[0], extra[1]]).is_err(),
                "{extra:?}"
            );
        }
        assert!(Args::try_parse_from(["md2docx", "in.md", "--untrusted", "--mermaid-engine", "mmdc"]).is_err());
        // Defaults for the normal mode keep images inside the input's directory.
        let d = parse(&[]);
        assert_eq!(config(&d).unwrap().image_access, LibImageAccess::BaseDir);
    }

    #[test]
    fn untrusted_limits_can_be_lowered_not_raised() {
        let lowered = config(&parse(&[
            "--untrusted",
            "--max-input-bytes",
            "1M",
            "--max-elements",
            "100",
        ]))
        .unwrap();
        assert_eq!(lowered.limits.max_input_bytes, 1 << 20);
        assert_eq!(lowered.limits.max_elements, 100);
        for flag in [
            ["--max-input-bytes", "1G"],
            ["--max-output-bytes", "1G"],
            ["--max-uncompressed-bytes", "2G"],
            ["--max-elements", "99999999"],
            ["--deadline", "100000"],
        ] {
            let err = config(&parse(&["--untrusted", flag[0], flag[1]])).unwrap_err();
            assert_eq!(err.exit.code(), 2, "{flag:?}: {}", err.message);
        }
        // Without --untrusted, any value is fine.
        let raised = config(&parse(&["--max-input-bytes", "1G"])).unwrap();
        assert_eq!(raised.limits.max_input_bytes, 1 << 30);
    }

    #[test]
    fn link_schemes_can_be_narrowed_under_untrusted_not_widened() {
        let c = config(&parse(&["--untrusted", "--link-schemes", "https"])).unwrap();
        assert_eq!(c.allowed_link_schemes, ["https"]);
        assert!(config(&parse(&["--untrusted", "--link-schemes", "https,ftp"])).is_err());
        let wide = config(&parse(&["--link-schemes", "https,ftp"])).unwrap();
        assert_eq!(wide.allowed_link_schemes, ["https", "ftp"]);
    }

    #[test]
    fn mermaid_engine_defaults_to_merman_and_can_switch_to_mmdc() {
        let d = parse(&[]);
        assert_eq!(config(&d).unwrap().graphs.mermaid.program, "merman-cli");
        let m = parse(&["--mermaid-engine", "mmdc"]);
        assert_eq!(config(&m).unwrap().graphs.mermaid.program, "mmdc");
        // An explicit command wins, and cannot be combined with the engine switch.
        let c = parse(&["--mermaid-command", "my-mmdc -i {input} -o {output}"]);
        assert_eq!(config(&c).unwrap().graphs.mermaid.program, "my-mmdc");
        assert!(Args::try_parse_from([
            "md2docx",
            "in.md",
            "--mermaid-engine",
            "mmdc",
            "--mermaid-command",
            "x -i {input} -o {output}"
        ])
        .is_err());
    }

    #[test]
    fn flags_map_onto_the_library_config() {
        let a = parse(&[
            "--math-mode",
            "image",
            "--dolphin-command",
            "d -i {input} -o {output}",
            "--timeout",
            "5",
            "--latex",
            "xelatex",
        ]);
        let c = config(&a).unwrap();
        assert_eq!(c.math.mode, LibMathMode::ImageFallback);
        assert_eq!(c.graphs.dolphin.program, "d");
        assert_eq!(c.tool_timeout, Duration::from_secs(5));
        assert!(matches!(&c.math.engine, MathImageEngine::LatexDvipng { latex, .. } if latex == "xelatex"));
        // Default is the safe, tool-free mode.
        assert_eq!(config(&parse(&[])).unwrap().math.mode, LibMathMode::NativeOmml);
    }

    #[test]
    fn document_flags_map_onto_the_document_options() {
        let c = config(&parse(&[
            "--title",
            "T",
            "--author",
            "A",
            "--page-size",
            "letter",
            "--landscape",
            "--margin",
            "20mm",
            "--body-font",
            "Georgia",
            "--body-size",
            "10.5",
            "--code-font",
            "Menlo",
            "--page-numbers",
            "--source-date-epoch",
            "1700000000",
        ]))
        .unwrap();
        let d = &c.document;
        assert_eq!((d.title.as_deref(), d.author.as_deref()), (Some("T"), Some("A")));
        assert_eq!(d.page.twips(), (15840, 12240));
        assert_eq!(d.margin_twips, 1134);
        assert_eq!((d.body_font.as_str(), d.body_size_half_points), ("Georgia", 21));
        assert_eq!(d.code_font, "Menlo");
        assert!(d.page_numbers);
        assert_eq!(d.timestamp, Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)));
    }

    #[test]
    fn a_bad_document_setting_is_a_usage_error() {
        let err = config(&parse(&["--margin", "20in"])).unwrap_err();
        assert_eq!(err.exit.code(), 2, "{}", err.message);
    }
}
