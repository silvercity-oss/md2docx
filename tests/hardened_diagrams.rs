//! `Config::untrusted_with_diagrams()`: Mermaid diagrams for untrusted input.
//!
//! These tests use stand-in tools and need no merman install. The real-tool
//! confinement checks are in `merman_hardened.rs`.

mod common;

use common::*;
use md2docx::__internals::preprocess;
use md2docx::{markdown_to_docx, Config, CrateError};

const MERMAID: &str = "```mermaid\ngraph TD; A-->B\n```\n";

/// The hardened preset pointed at a path that does not exist, so no process can
/// ever start (the stem must still be `merman-cli` to pass validation).
fn hardened_without_tool() -> Config {
    let mut cfg = Config::untrusted_with_diagrams();
    cfg.graphs.mermaid.program = "/nonexistent/merman-cli".into();
    cfg
}

#[test]
fn dolphin_fences_stay_code_in_hardened_mode() {
    // Dolphin is an arbitrary configured command; hardened mode never runs it.
    let mut cfg = hardened_without_tool();
    cfg.graphs.dolphin.program = "/nonexistent/must-never-run-dolphin".into();
    let md = "```dolphin\nnode a -> node b\n```\n";
    let pre = preprocess(md, &cfg).unwrap();
    assert_eq!(pre.markdown, md, "left untouched");
    assert!(pre.assets.is_empty());
    let b = body_with(md, &cfg);
    assert!(visible_text(&b).contains("node a -> node b"));
    assert!(b.contains("CodeBlock"));
}

#[test]
fn a_missing_merman_is_reported_with_install_instructions() {
    match markdown_to_docx(MERMAID, &hardened_without_tool()) {
        Err(CrateError::MissingExternalCli { program }) => assert_eq!(program, "/nonexistent/merman-cli"),
        other => panic!("expected MissingExternalCli, got {:?}", other.map(|d| d.len())),
    }
    let msg = CrateError::MissingExternalCli {
        program: "merman-cli".into(),
    }
    .to_string();
    assert!(msg.contains("cargo install merman-cli"), "{msg}");
}

#[test]
fn diagram_count_is_capped_before_anything_runs() {
    let cfg = hardened_without_tool(); // limit: 20 diagrams; no tool, so a render attempt would be a different error
    let many: String = (0..21)
        .map(|i| format!("```mermaid\ngraph TD; A{i}-->B\n```\n\n"))
        .collect();
    assert!(
        matches!(markdown_to_docx(&many, &cfg), Err(CrateError::LimitExceeded(m)) if m.contains("diagrams")),
        "21 diagrams must be refused up front"
    );
    // The same limit exists for the default configuration (100).
    let default_many: String = (0..101)
        .map(|i| format!("```mermaid\ngraph TD; A{i}-->B\n```\n\n"))
        .collect();
    assert!(matches!(
        markdown_to_docx(&default_many, &Config::trusted()),
        Err(CrateError::LimitExceeded(m)) if m.contains("diagrams")
    ));
}

#[test]
fn oversized_diagram_source_is_refused() {
    let cfg = hardened_without_tool(); // 16 KiB per diagram
    let big = format!("```mermaid\ngraph TD\n{}\n```\n", "  A --> B\n".repeat(2000));
    assert!(big.len() > 16 * 1024);
    assert!(matches!(
        markdown_to_docx(&big, &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("diagram")
    ));
}

#[test]
fn hardened_mode_rejects_any_command_but_the_stock_merman_line() {
    let tweaks: [fn(&mut Config); 4] = [
        |c| c.graphs.mermaid.program = "mmdc".into(),
        |c| c.graphs.mermaid.program = "/bin/sh".into(),
        |c| c.graphs.mermaid.args.push("--allow-network".into()),
        |c| c.graphs.mermaid.args.retain(|a| a != "--input-kind" && a != "mermaid"),
    ];
    for tweak in tweaks {
        let mut cfg = Config::untrusted_with_diagrams();
        tweak(&mut cfg);
        assert!(
            matches!(markdown_to_docx(MERMAID, &cfg), Err(CrateError::InvalidConfig(_))),
            "{:?}",
            cfg.graphs.mermaid
        );
    }
    // Plain `untrusted()` has diagrams off entirely and starts nothing.
    let b = body_with(MERMAID, &Config::untrusted());
    assert!(visible_text(&b).contains("graph TD; A-->B"));
}

/// The environment a hardened tool sees: nothing from the host process.
#[cfg(unix)]
#[test]
fn hardened_tool_runs_with_a_scrubbed_environment() {
    let fx = Fixture::new();
    let dump = fx.path("env.dump");
    let tool = |name_dir: &str| {
        let dir = fx.path(name_dir);
        std::fs::create_dir_all(&dir).unwrap();
        script(
            &dir,
            "merman-cli", // the stem must be `merman-cli` to pass hardened validation
            &format!(
                "while [ $# -gt 0 ]; do case \"$1\" in -o) OUT=\"$2\"; shift 2;; *) shift;; esac; done\nenv > '{}'\ncp '{}' \"$OUT\"",
                dump.display(),
                fx.path("fixture.png").display()
            ),
        )
    };
    let keys = |dump_path: &std::path::Path| -> Vec<(String, String)> {
        std::fs::read_to_string(dump_path)
            .unwrap()
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect()
    };

    // Hardened: only PATH-like basics, with HOME/TMPDIR pointing into the scratch directory.
    let mut cfg = Config::untrusted_with_diagrams();
    cfg.graphs.mermaid.program = tool("hardened");
    markdown_to_docx(MERMAID, &cfg).expect("mock tool should be accepted and run");
    let env = keys(&dump);
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    let allowed = [
        "PATH",
        "HOME",
        "TMPDIR",
        "TEMP",
        "TMP",
        "LANG",
        "LC_ALL",
        "PWD",
        "SHLVL",
        "_",
        "OLDPWD",
        "PATHEXT",
        // Added by macOS to every process it starts from a cleared environment.
        "__CF_USER_TEXT_ENCODING",
    ];
    for n in &names {
        assert!(
            allowed.contains(n),
            "unexpected variable `{n}` leaked into the hardened tool: {names:?}"
        );
    }
    assert!(
        !names.iter().any(|n| n.starts_with("CARGO") || n.starts_with("RUST")),
        "{names:?}"
    );
    let home = &env.iter().find(|(k, _)| k == "HOME").expect("HOME is set").1;
    assert!(
        home.contains("md2docx-"),
        "HOME should be the scratch directory, got {home}"
    );
    assert_eq!(env.iter().find(|(k, _)| k == "TMPDIR").map(|(_, v)| v), Some(home));

    // Control: the same tool under a normal configuration inherits the host environment,
    // so the check above is really observing the scrub and not an empty environment.
    std::fs::remove_file(&dump).unwrap();
    let mut normal = Config::native_math();
    normal.graphs.mermaid.program = tool("normal");
    normal.graphs.mermaid.args = ["-o", "{output}", "{input}"].map(String::from).to_vec();
    markdown_to_docx(MERMAID, &normal).unwrap();
    let inherited = keys(&dump);
    assert!(
        inherited.iter().any(|(k, _)| k.starts_with("CARGO")),
        "control: a normal run should inherit cargo's variables"
    );
    assert!(inherited.len() > env.len() + 5);
}
