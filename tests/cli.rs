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

#[test]
fn highlight_flag() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("h.md");
    std::fs::write(&input, "```rust\nfn main() {}\n```\n").unwrap();
    let doc = |extra: &[&str]| {
        let out = dir.path().join(format!("h{}.docx", extra.join("")));
        let run = Command::new(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .arg("-o")
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        (run, out)
    };
    let (run, out) = doc(&[]);
    assert!(run.status.success());
    let plain = zip_text(&std::fs::read(&out).unwrap(), "word/document.xml");
    assert!(plain.contains("F2F2F2") && !plain.contains("569CD6"));
    let (run, out) = doc(&["--highlight", "dark"]);
    assert!(run.status.success());
    let dark = zip_text(&std::fs::read(&out).unwrap(), "word/document.xml");
    assert!(dark.contains("1E1E1E") && dark.contains("569CD6"));
    let (run, out) = doc(&["--highlight", "light", "--untrusted"]);
    assert!(run.status.success());
    assert!(zip_text(&std::fs::read(&out).unwrap(), "word/document.xml").contains("0000FF"));
    let (run, _) = doc(&["--highlight", "neon"]);
    assert_eq!(run.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn output_permissions_follow_the_umask_and_existing_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("p.md");
    std::fs::write(&input, "# p\n").unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let run = |umask: &str, out: &Path| {
        Command::new("sh")
            .arg("-c")
            .arg(format!("umask {umask}; exec \"$0\" \"$1\" -o \"$2\""))
            .arg(env!("CARGO_BIN_EXE_md2docx"))
            .arg(&input)
            .arg(out)
            .output()
            .unwrap()
    };
    // A new file: 0666 less the umask.
    let fresh = dir.path().join("fresh.docx");
    assert!(run("077", &fresh).status.success());
    assert_eq!(mode(&fresh), 0o600);
    let open = dir.path().join("open.docx");
    assert!(run("022", &open).status.success());
    assert_eq!(mode(&open), 0o644);
    // An existing file keeps its permissions.
    let private = dir.path().join("private.docx");
    std::fs::write(&private, b"old").unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(run("022", &private).status.success());
    assert_eq!(mode(&private), 0o600);
    assert!(std::fs::read(&private).unwrap().starts_with(b"PK"));
}

// ---- strict mode and diagnostics

#[test]
fn findings_are_printed_with_line_and_column_and_do_not_stop_a_permissive_run() {
    let fx = Fixture::new();
    let input = fx.path("doc.md");
    std::fs::write(&input, "# Title\n\nSee [the spec](docs/spec.md) for details.\n").unwrap();
    let out = md2docx(&[path_str(&input)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("doc.md:3:5: warning[link.dropped]"), "{err}");
    assert!(fx.path("doc.docx").exists());
}

#[test]
fn strict_refuses_a_lossy_document_lists_everything_and_writes_nothing() {
    let fx = Fixture::new();
    let input = fx.path("lossy.md");
    std::fs::write(
        &input,
        "[a](gone.md)\n\n![remote](https://example.com/x.png)\n\n```mermaid\ngraph TD\n```\n",
    )
    .unwrap();
    let out = md2docx(&["--strict", "--untrusted", path_str(&input)]);
    assert!(!out.status.success());
    let err = stderr(&out);
    for code in ["link.dropped", "image.remote", "diagram.not-rendered"] {
        assert!(err.contains(&format!("[{code}]")), "{code} missing:\n{err}");
    }
    assert!(err.contains("nothing was written"), "{err}");
    assert!(!fx.path("lossy.docx").exists());
    // A clean document passes strict.
    let clean = fx.path("clean.md");
    std::fs::write(&clean, "# Fine\n\nJust text and $x^2$.\n").unwrap();
    assert!(md2docx(&["--strict", path_str(&clean)]).status.success());
}

#[test]
fn the_scratch_directory_option_moves_the_working_directory() {
    let fx = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    let input = fx.path("m.md");
    std::fs::write(&input, "# Math\n\n$$x$$\n").unwrap();
    assert!(md2docx(&["--scratch-dir", path_str(root.path()), path_str(&input)])
        .status
        .success());
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        0,
        "scratch removed afterwards"
    );
    let missing = md2docx(&["--scratch-dir", "/definitely/not/there", path_str(&input)]);
    // Nothing needs files here (native math), so a bad root is not touched ...
    assert!(missing.status.success());
    // ... but a document that does need scratch space says so.
    let fenced = fx.path("d.md");
    std::fs::write(&fenced, "```mermaid\ngraph TD; A-->B\n```\n").unwrap();
    let bad = md2docx(&["--scratch-dir", "/definitely/not/there", path_str(&fenced)]);
    assert!(!bad.status.success());
}

#[cfg(unix)]
#[test]
fn a_force_killed_conversion_leaves_only_what_sweep_scratch_can_recover() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let fx = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    let pidfile = fx.path("tool.pid");
    let tool = script(
        fx.dir.path(),
        "slow-mermaid",
        &format!("echo $$ > '{}'\nexec sleep 300", pidfile.display()),
    );
    let input = fx.path("slow.md");
    std::fs::write(&input, "```mermaid\ngraph TD; A-->B\n```\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_md2docx"))
        .args([
            "--scratch-dir",
            path_str(root.path()),
            "--mermaid-command",
            &format!("{tool} {{input}} {{output}}"),
            path_str(&input),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !pidfile.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(pidfile.exists(), "the tool never started");
    // Forced termination: no destructor runs.
    child.kill().unwrap();
    child.wait().unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(root.path()).unwrap().flatten().collect();
    assert_eq!(leftovers.len(), 1, "the job directory is what recovery has to find");
    assert!(leftovers[0].file_name().to_string_lossy().starts_with("silvercity-"));

    // Recovery by the host, then reaping what the killed worker left running (a real host does
    // both: the scratch directory is its own, and a cgroup or job object ends the processes).
    assert_eq!(silvercity_oss::sweep_scratch(root.path(), Duration::ZERO).unwrap(), 1);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let pid = std::fs::read_to_string(&pidfile).unwrap().trim().to_string();
    let _ = Command::new("kill").args(["-9", &pid]).status();
}

#[test]
fn layout_flags_reach_the_document() {
    let fx = Fixture::new();
    let input = fx.path("book.md");
    std::fs::write(&input, "# One\n\n## Two\n\nSee https://example.com and H ~2~ O.\n").unwrap();
    let out = md2docx(&[
        "--toc",
        "2",
        "--number-headings",
        "--autolink",
        "--sup-sub",
        path_str(&input),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let docx = std::fs::read(fx.path("book.docx")).unwrap();
    let body = zip_text(&docx, "word/document.xml");
    assert!(visible_text(&body).starts_with("Contents"), "{body}");
    assert!(body.contains("<w:numPr>") && body.contains("w:val=\"subscript\""));
    assert!(zip_text(&docx, "word/_rels/document.xml.rels").contains("https://example.com"));
}

#[test]
fn a_reference_document_is_read_and_checked() {
    let fx = Fixture::new();
    let input = fx.path("r.md");
    std::fs::write(&input, "# Styled\n").unwrap();
    // The converter's own output makes a fine reference.
    assert!(md2docx(&[path_str(&input)]).status.success());
    let reference = fx.path("brand.docx");
    std::fs::copy(fx.path("r.docx"), &reference).unwrap();
    let out = md2docx(&[
        "--reference-doc",
        path_str(&reference),
        "-o",
        path_str(&fx.path("out.docx")),
        path_str(&input),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let bad = md2docx(&["--reference-doc", path_str(&input), path_str(&input)]);
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("reference document"), "{}", stderr(&bad));
}
