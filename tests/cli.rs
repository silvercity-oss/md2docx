//! Tests for the `md2docx` binary (clap interface), run as a real subprocess.

#![cfg(feature = "cli")]

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::*;

fn md2docx(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .args(args)
        .output()
        .expect("md2docx should start")
}

fn path_str(p: &Path) -> &str {
    p.to_str().expect("utf-8 temp path")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn help_and_version_describe_the_interface() {
    let help = md2docx(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout).into_owned();
    for flag in [
        "--output",
        "--math-mode",
        "--dolphin-command",
        "--mermaid-command",
        "--latex",
        "--dvipng",
        "--timeout",
    ] {
        assert!(text.contains(flag), "--help is missing {flag}:\n{text}");
    }
    assert!(text.contains("native") && text.contains("image"));
    assert!(
        text.contains("SECURITY"),
        "the untrusted-input warning belongs in --help"
    );

    let version = md2docx(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("md2docx "));
}

#[test]
fn converts_with_native_math_by_default_and_derives_the_output_path() {
    let fx = Fixture::new();
    let input = fx.path("notes.md");
    std::fs::write(&input, "# Notes\n\nEuler: $e^{i\\pi}+1=0$\n\n$$\\frac{a}{b}$$\n").unwrap();

    let out = md2docx(&[path_str(&input)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let docx = std::fs::read(fx.path("notes.docx")).expect("default output is <input>.docx");
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(body.contains("<m:oMathPara>") && body.contains("<m:sSup>"));
    assert_eq!(media_count(&docx), 0, "native mode embeds no pictures");
}

#[test]
fn explicit_output_path_is_honoured() {
    let fx = Fixture::new();
    let input = fx.path("a.md");
    std::fs::write(&input, "hello\n").unwrap();
    let target = fx.path("custom.docx");
    let out = md2docx(&[path_str(&input), "-o", path_str(&target)]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(&std::fs::read(target).unwrap()[..2], b"PK");
}

#[test]
fn image_mode_reports_the_missing_tex_binary() {
    let fx = Fixture::new();
    let input = fx.path("m.md");
    std::fs::write(&input, "$x$\n").unwrap();
    let out = md2docx(&[
        path_str(&input),
        "--math-mode",
        "image",
        "--latex",
        "no-such-latex-binary",
    ]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("no-such-latex-binary"), "{}", stderr(&out));
    assert!(!fx.path("m.docx").exists(), "no partial output on failure");
}

#[test]
fn invalid_arguments_are_rejected_by_clap() {
    let fx = Fixture::new();
    let input = fx.path("a.md");
    std::fs::write(&input, "x\n").unwrap();
    let input = path_str(&input);

    let no_placeholders = md2docx(&[input, "--dolphin-command", "dolphin --render"]);
    assert_eq!(no_placeholders.status.code(), Some(2));
    assert!(
        stderr(&no_placeholders).contains("{input}"),
        "{}",
        stderr(&no_placeholders)
    );

    let unbalanced = md2docx(&[input, "--mermaid-command", "mmdc 'oops -i {input} -o {output}"]);
    assert_eq!(unbalanced.status.code(), Some(2));

    let bad_mode = md2docx(&[input, "--math-mode", "latex"]);
    assert_eq!(bad_mode.status.code(), Some(2));
    assert!(
        stderr(&bad_mode).contains("native"),
        "clap should list the valid values"
    );

    assert_eq!(md2docx(&[]).status.code(), Some(2), "input is required");
}

#[test]
fn runtime_errors_exit_nonzero_with_a_message() {
    let fx = Fixture::new();
    let missing = fx.path("nope.md");
    let out = md2docx(&[path_str(&missing)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("error:") && stderr(&out).contains("nope.md"),
        "{}",
        stderr(&out)
    );

    let bad = fx.path("bad.md");
    std::fs::write(&bad, "$$\\left( x$$\n").unwrap();
    let out = md2docx(&[path_str(&bad)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("line 1"), "{}", stderr(&out));

    let same = md2docx(&[path_str(&bad), "-o", path_str(&bad)]);
    assert_eq!(same.status.code(), Some(1));
    assert!(stderr(&same).contains("overwrite"));
    assert_eq!(
        std::fs::read_to_string(&bad).unwrap(),
        "$$\\left( x$$\n",
        "input must be untouched"
    );
}

#[cfg(unix)]
#[test]
fn dolphin_command_flag_drives_a_custom_renderer() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fixture::new();
    let tool = fx.path("fake-dolphin");
    // Called as: fake-dolphin --theme dark -i IN -o OUT
    std::fs::write(
        &tool,
        format!(
            "#!/bin/sh\necho \"$@\" > '{log}'\nwhile [ $# -gt 0 ]; do case \"$1\" in -o) OUT=\"$2\"; shift 2;; *) shift;; esac; done\ncp '{png}' \"$OUT\"\n",
            log = fx.path("args.log").display(),
            png = fx.path("fixture.png").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

    let input = fx.path("g.md");
    std::fs::write(&input, "```dolphin\nnode a -> node b\n```\n").unwrap();
    let template = format!("{} --theme dark -i {{input}} -o {{output}}", tool.display());
    let out = md2docx(&[path_str(&input), "--dolphin-command", &template]);
    assert!(out.status.success(), "{}", stderr(&out));

    let args = std::fs::read_to_string(fx.path("args.log")).unwrap();
    assert!(
        args.starts_with("--theme dark -i ") && args.contains(".dolphin -o "),
        "{args}"
    );
    assert_eq!(media_count(&std::fs::read(fx.path("g.docx")).unwrap()), 1);
}

#[test]
fn untrusted_flag_converts_hostile_input_without_side_effects() {
    let fx = Fixture::new();
    let input = fx.path("hostile.md");
    std::fs::write(
        &input,
        "# Upload\n\n```mermaid\ngraph TD; A-->B\n```\n\n$$\\frac{a}{b}$$\n\n![pic](fixture.png)\n\n\
         [click](javascript:alert(1)) <script>alert(2)</script>\n",
    )
    .unwrap();
    let out = md2docx(&[path_str(&input), "--untrusted"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let docx = std::fs::read(fx.path("hostile.docx")).unwrap();
    assert_eq!(media_count(&docx), 0);
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert!(body.contains("<m:oMathPara>"));
    let text = visible_text(&body);
    assert!(text.contains("graph TD; A-->B") && text.contains("[pic]") && text.contains("click"));
    assert!(!text.contains("alert"));
    let rels = zip_text(&docx, "word/_rels/document.xml.rels");
    assert!(!rels.contains("javascript"));

    // Loosening switches cannot be combined with it.
    for extra in [
        ["--math-mode", "image"],
        ["--image-access", "any"],
        ["--latex", "xelatex"],
    ] {
        let o = md2docx(&[path_str(&input), "--untrusted", extra[0], extra[1]]);
        assert_eq!(o.status.code(), Some(2), "{extra:?}");
    }
}

#[test]
fn images_are_confined_to_the_input_directory_by_default() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.path("docs")).unwrap();
    std::fs::copy(fx.path("fixture.png"), fx.path("docs/inside.png")).unwrap();
    let input = fx.path("docs/page.md");
    std::fs::write(&input, "![in](inside.png)\n\n![out](../fixture.png)\n").unwrap();
    let out = md2docx(&[path_str(&input)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("outside the base directory"), "{}", stderr(&out));

    let ok = md2docx(&[path_str(&input), "--image-access", "any"]);
    assert!(ok.status.success(), "{}", stderr(&ok));
    assert_eq!(
        media_count(&std::fs::read(fx.path("docs/page.docx")).unwrap()),
        1,
        "same bytes, stored once"
    );

    let none = md2docx(&[
        path_str(&input),
        "--image-access",
        "none",
        "-o",
        path_str(&fx.path("none.docx")),
    ]);
    assert!(none.status.success(), "{}", stderr(&none));
    assert_eq!(media_count(&std::fs::read(fx.path("none.docx")).unwrap()), 0);
}

#[test]
fn oversized_input_is_refused_with_a_clear_message() {
    let fx = Fixture::new();
    let input = fx.path("big.md");
    // Just over the --untrusted 2 MiB ceiling.
    std::fs::write(&input, "word ".repeat(2 * 1024 * 1024 / 5 + 10)).unwrap();
    let out = md2docx(&[path_str(&input), "--untrusted"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("limit exceeded"), "{}", stderr(&out));
    assert!(!fx.path("big.docx").exists());
}

#[test]
fn mermaid_engine_flag_names_the_missing_tool_and_how_to_get_it() {
    if have("merman-cli") || have("mmdc") {
        return; // the point is the error for a missing tool
    }
    let fx = Fixture::new();
    let input = fx.path("d.md");
    std::fs::write(&input, "```mermaid\ngraph TD; A-->B\n```\n").unwrap();
    let default = md2docx(&[path_str(&input)]);
    assert_eq!(default.status.code(), Some(1));
    assert!(
        stderr(&default).contains("merman-cli") && stderr(&default).contains("cargo install merman-cli"),
        "{}",
        stderr(&default)
    );
    let mmdc = md2docx(&[path_str(&input), "--mermaid-engine", "mmdc"]);
    assert_eq!(mmdc.status.code(), Some(1));
    assert!(
        stderr(&mmdc).contains("`mmdc`") && stderr(&mmdc).contains("mermaid-cli"),
        "{}",
        stderr(&mmdc)
    );
    assert!(!fx.path("d.docx").exists());
}

#[test]
fn compression_level_flag() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("c.md");
    std::fs::write(
        &input,
        "```\n".to_string() + &"repeat repeat repeat\n".repeat(2000) + "```\n",
    )
    .unwrap();
    let run = |extra: &[&str]| {
        let out = dir.path().join(format!("o{}.docx", extra.join("")));
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        (status, out)
    };
    let (ok, default) = run(&[]);
    assert!(ok.status.success());
    let (ok, stored) = run(&["--compression-level", "0"]);
    assert!(ok.status.success());
    assert!(std::fs::metadata(&default).unwrap().len() * 4 < std::fs::metadata(&stored).unwrap().len());
    let (bad, _) = run(&["--compression-level", "10"]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn code_blocks_flag() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("c.md");
    std::fs::write(&input, "```\none\ntwo\nthree\n```\n").unwrap();
    let docx_for = |extra: &[&str]| {
        let out = dir.path().join(format!("c{}.docx", extra.join("")));
        let run = std::process::Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        (run, out)
    };
    let paragraphs = |path: &Path| {
        zip_text(&std::fs::read(path).unwrap(), "word/document.xml")
            .matches("w:val=\"CodeBlock\"")
            .count()
    };
    let (run, out) = docx_for(&[]);
    assert!(run.status.success());
    assert_eq!(paragraphs(&out), 3);
    let (run, out) = docx_for(&["--code-blocks", "single"]);
    assert!(run.status.success());
    assert_eq!(paragraphs(&out), 1);
    let (run, _) = docx_for(&["--code-blocks", "wat"]);
    assert_eq!(run.status.code(), Some(2));
}

#[test]
fn a_bare_file_name_resolves_images_next_to_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("img.png"), png_bytes(30, 20)).unwrap();
    std::fs::write(dir.path().join("notes.md"), "# Notes\n\n![pic](img.png)\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .current_dir(dir.path())
        .arg("notes.md")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let docx = std::fs::read(dir.path().join("notes.docx")).unwrap();
    assert_eq!(media_count(&docx), 1);
}

#[test]
fn the_input_is_never_overwritten_however_the_output_is_spelled() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("notes.md");
    std::fs::write(&input, "# keep me\n").unwrap();
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut spellings: Vec<std::path::PathBuf> =
        vec![dir.path().join("./notes.md"), dir.path().join("sub/../notes.md")];
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&input, dir.path().join("link.docx")).unwrap();
        spellings.push(dir.path().join("link.docx"));
        std::fs::hard_link(&input, dir.path().join("hard.docx")).unwrap();
        spellings.push(dir.path().join("hard.docx"));
    }
    for out in spellings {
        let run = Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(1), "{}", out.display());
        assert!(String::from_utf8_lossy(&run.stderr).contains("refusing"));
        assert_eq!(
            std::fs::read_to_string(&input).unwrap(),
            "# keep me\n",
            "{}",
            out.display()
        );
    }
}

#[test]
fn output_is_written_atomically_with_ordinary_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.md");
    let out = dir.path().join("a.docx");
    std::fs::write(&out, b"previous output").unwrap();
    // A conversion that fails leaves the existing output alone and no temporary files behind.
    std::fs::write(&input, "bad \0 input").unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(!run.status.success());
    assert_eq!(std::fs::read(&out).unwrap(), b"previous output");
    // A good one replaces it completely.
    std::fs::write(&input, "# ok\n").unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(run.status.success());
    assert!(std::fs::read(&out).unwrap().starts_with(b"PK"));
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".md2docx-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o644);
    }
}

#[test]
fn the_untrusted_diagram_timeout_cannot_be_raised() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.md");
    std::fs::write(&input, "plain\n").unwrap();
    let run = |secs: &str| {
        Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .args(["--untrusted", "--diagrams", "--timeout", secs])
            .output()
            .unwrap()
    };
    assert_eq!(run("21").status.code(), Some(2));
    assert!(run("20").status.success());
    assert!(run("5").status.success());
}
