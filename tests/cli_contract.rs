//! The command-line contract: exit status, standard streams, the JSON report, environment
//! variables, batch and check modes. `docs/cli.md` is the specification; these tests are what
//! keeps it true. Exit statuses are part of the interface and are asserted by number on purpose.

#![cfg(feature = "cli")]

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::*;

fn cmd(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_md2docx-cli"));
    c.args(args);
    // A developer's own settings must not leak into the tests.
    for var in [
        "MD2DOCX_FORMAT",
        "MD2DOCX_QUIET",
        "MD2DOCX_STRICT",
        "MD2DOCX_UNTRUSTED",
        "MD2DOCX_DEADLINE",
        "MD2DOCX_TIMEOUT",
        "MD2DOCX_BASE_DIR",
        "MD2DOCX_SCRATCH_DIR",
        "MD2DOCX_MAX_INPUT_BYTES",
        "MD2DOCX_MAX_OUTPUT_BYTES",
        "MD2DOCX_MAX_UNCOMPRESSED_BYTES",
        "MD2DOCX_MAX_ELEMENTS",
        "MD2DOCX_MAX_IMAGE_BYTES",
        "MD2DOCX_MAX_IMAGES",
        "SOURCE_DATE_EPOCH",
    ] {
        c.env_remove(var);
    }
    c
}

fn run(args: &[&str]) -> Output {
    cmd(args).output().expect("md2docx-cli should start")
}

fn run_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = cmd(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn write(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

const LOSSY: &str = "[a](relative/path.md) and ![x](https://example.com/x.png)\n";

// ------------------------------------------------------------------ exit status

#[test]
fn exit_status_0_means_converted_even_with_warnings() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let out = run(&[p(&lossy)]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(err(&out).contains("warning[link.dropped]"), "{}", err(&out));
    assert!(fx.dir.path().join("lossy.docx").exists());
}

#[test]
fn exit_status_2_is_usage() {
    let fx = Fixture::new();
    let a = write(fx.dir.path(), "a.md", "a\n");
    let b = write(fx.dir.path(), "b.md", "b\n");
    for args in [
        vec!["--no-such-flag", p(&a)],
        vec![],
        vec![p(&a), p(&b), "-o", p(&fx.dir.path().join("one.docx"))], // several inputs, one file
        vec![p(&a), "-"],                                             // stdin among others
        vec![p(&a), "--check", "-o", p(&fx.dir.path().join("x.docx"))], // nothing to write to
        vec![p(&a), "--untrusted", "--max-input-bytes", "1G"],        // cannot raise a limit
        vec![p(&a), "--margin", "20in"],                              // impossible layout
        vec![p(&a), "-o", p(&a)],                                     // would overwrite the input
    ] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", err(&out));
    }
}

#[test]
fn exit_status_3_is_file_io() {
    let fx = Fixture::new();
    let a = write(fx.dir.path(), "a.md", "a\n");
    let missing = run(&[p(&fx.dir.path().join("nope.md"))]);
    assert_eq!(missing.status.code(), Some(3), "{}", err(&missing));
    let unwritable = run(&[p(&a), "-o", p(&fx.dir.path().join("no/such/dir/out.docx"))]);
    assert_eq!(unwritable.status.code(), Some(3), "{}", err(&unwritable));
    let directory = run(&[p(fx.dir.path())]);
    assert_eq!(directory.status.code(), Some(3), "{}", err(&directory));
}

#[test]
fn exit_status_4_is_a_rejected_document() {
    let fx = Fixture::new();
    let bad_math = write(fx.dir.path(), "m.md", "$$\\left( x$$\n");
    let big = write(fx.dir.path(), "big.md", &"word ".repeat(100));
    let outside = write(fx.dir.path(), "i.md", "![x](../../../../etc/passwd)\n");
    assert_eq!(run(&[p(&bad_math)]).status.code(), Some(4));
    assert_eq!(run(&[p(&big), "--max-input-bytes", "100"]).status.code(), Some(4));
    assert_eq!(run(&[p(&outside)]).status.code(), Some(4));
    let not_utf8 = fx.dir.path().join("bin.md");
    std::fs::write(&not_utf8, [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(run(&[p(&not_utf8)]).status.code(), Some(4));
}

#[test]
fn exit_status_5_is_a_strict_refusal_and_nothing_is_written() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let out = run(&["--strict", p(&lossy)]);
    assert_eq!(out.status.code(), Some(5), "{}", err(&out));
    assert!(!fx.dir.path().join("lossy.docx").exists());
}

#[test]
fn exit_status_6_is_an_external_tool() {
    let fx = Fixture::new();
    let d = write(fx.dir.path(), "d.md", "```mermaid\ngraph TD; A-->B\n```\n");
    let out = run(&[
        p(&d),
        "--mermaid-command",
        "definitely-not-installed-xyz -i {input} -o {output}",
    ]);
    assert_eq!(out.status.code(), Some(6), "{}", err(&out));
    // ... unless the author asked for such a block to be kept as code.
    let kept = run(&[
        p(&d),
        "--mermaid-command",
        "definitely-not-installed-xyz -i {input} -o {output}",
        "--keep-failed-diagrams",
    ]);
    assert_eq!(kept.status.code(), Some(0), "{}", err(&kept));
}

#[cfg(unix)]
#[test]
fn exit_status_7_is_the_deadline() {
    let fx = Fixture::new();
    // A renderer that outlasts the deadline.
    let d = write(fx.dir.path(), "d.md", "```mermaid\ngraph TD; A-->B\n```\n");
    let started = std::time::Instant::now();
    let out = run(&[
        p(&d),
        "--mermaid-command",
        "sh -c 'sleep 30' {input} {output}",
        "--deadline",
        "1",
    ]);
    assert_eq!(out.status.code(), Some(7), "{}", err(&out));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    // A zero deadline is a configuration mistake, not a timeout.
    let zero = run(&[p(&d), "--deadline", "0"]);
    assert_eq!(zero.status.code(), Some(2), "{}", err(&zero));
}

// ------------------------------------------------------------------ streams

#[test]
fn standard_input_to_standard_output() {
    let out = run_stdin(&["-"], b"# Piped\n\nHello.\n");
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(out.stdout.starts_with(b"PK"), "a .docx on stdout");
    assert!(zip_text(&out.stdout, "word/document.xml").contains("Piped"));
    assert!(err(&out).is_empty(), "nothing else is printed: {}", err(&out));
}

#[test]
fn standard_input_to_a_file_and_the_file_form_of_dash_o() {
    let fx = Fixture::new();
    let target = fx.dir.path().join("out.docx");
    let out = run_stdin(&["-", "-o", p(&target)], b"text\n");
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(out.stdout.is_empty());
    assert!(target.exists());
    let a = write(fx.dir.path(), "a.md", "# From a file\n");
    let to_stdout = run(&[p(&a), "-o", "-"]);
    assert_eq!(to_stdout.status.code(), Some(0));
    assert!(to_stdout.stdout.starts_with(b"PK"));
    assert!(!fx.dir.path().join("a.docx").exists());
}

#[test]
fn standard_input_obeys_the_input_limit_and_must_be_utf8() {
    let big = run_stdin(&["-", "--max-input-bytes", "50"], "word ".repeat(100).as_bytes());
    assert_eq!(big.status.code(), Some(4), "{}", err(&big));
    let bad = run_stdin(&["-"], &[0xff, 0xfe]);
    assert_eq!(bad.status.code(), Some(4), "{}", err(&bad));
}

// ------------------------------------------------------------------ check and quiet

#[test]
fn check_validates_without_writing() {
    let fx = Fixture::new();
    let good = write(fx.dir.path(), "good.md", "# Fine\n");
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let ok = run(&["--check", p(&good)]);
    assert_eq!(ok.status.code(), Some(0));
    assert!(err(&ok).contains("no problems"), "{}", err(&ok));
    let warned = run(&["--check", p(&lossy)]);
    assert_eq!(warned.status.code(), Some(0));
    assert!(err(&warned).contains("warning(s)"), "{}", err(&warned));
    let strict = run(&["--check", "--strict", p(&lossy)]);
    assert_eq!(strict.status.code(), Some(5), "{}", err(&strict));
    let stdin = run_stdin(&["--check", "-"], b"$$\\left( x$$\n");
    assert_eq!(stdin.status.code(), Some(4));
    assert!(!fx.dir.path().join("good.docx").exists() && !fx.dir.path().join("lossy.docx").exists());
}

#[test]
fn quiet_prints_no_warnings_and_no_wrote_line_but_errors_stay() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let out = run(&["-q", p(&lossy)]);
    assert_eq!(out.status.code(), Some(0));
    assert!(err(&out).is_empty(), "{}", err(&out));
    let strict = run(&["-q", "--strict", p(&lossy)]);
    assert_eq!(strict.status.code(), Some(5));
    assert!(
        err(&strict).contains("link.dropped") && err(&strict).contains("error:"),
        "{}",
        err(&strict)
    );
}

// ------------------------------------------------------------------ json

fn json_lines(o: &Output) -> Vec<String> {
    err(o).lines().map(str::to_string).collect()
}

#[test]
fn json_reports_one_object_per_input_and_nothing_else() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let out = run(&["--format", "json", p(&lossy)]);
    assert_eq!(out.status.code(), Some(0));
    let lines = json_lines(&out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let j = &lines[0];
    assert!(j.starts_with("{\"schema\":1,\"input\":\""), "{j}");
    assert!(j.contains("\"status\":\"ok\",\"exit_code\":0,\"error\":null"), "{j}");
    assert!(j.contains("\"code\":\"link.dropped\",\"severity\":\"warning\""), "{j}");
    assert!(j.contains("\"span\":{\"start\":"), "{j}");
    assert!(j.contains("\"stats\":{\"headings\":0"), "{j}");
    assert!(j.contains("lossy.docx"), "{j}");
}

#[test]
fn json_errors_carry_the_exit_code_a_stable_kind_and_the_strict_findings() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let strict = run(&["--format", "json", "--strict", p(&lossy)]);
    assert_eq!(strict.status.code(), Some(5));
    let j = &json_lines(&strict)[0];
    assert!(
        j.contains("\"status\":\"error\",\"exit_code\":5,\"error\":{\"kind\":\"lossy_content\""),
        "{j}"
    );
    assert!(j.contains("\"code\":\"link.dropped\""), "{j}");
    assert!(j.contains("\"output\":null") && j.contains("\"stats\":null"), "{j}");

    let missing = run(&["--format", "json", p(&fx.dir.path().join("nope.md"))]);
    assert_eq!(missing.status.code(), Some(3));
    assert!(json_lines(&missing)[0].contains("\"kind\":\"io\""));

    let usage = run(&["--format", "json", "--untrusted", "--max-input-bytes", "1G", p(&lossy)]);
    assert_eq!(usage.status.code(), Some(2));
    let j = &json_lines(&usage)[0];
    assert!(j.contains("\"input\":null") && j.contains("\"kind\":\"usage\""), "{j}");

    let bad = run_stdin(&["--format", "json", "-"], b"$$\\left( x$$\n");
    assert_eq!(bad.status.code(), Some(4));
    let j = &json_lines(&bad)[0];
    assert!(
        j.contains("\"input\":\"-\"") && j.contains("\"kind\":\"invalid_markdown\""),
        "{j}"
    );
}

#[test]
fn json_and_standard_output_do_not_mix() {
    let out = run_stdin(&["--format", "json", "-"], b"# T\n");
    assert!(out.stdout.starts_with(b"PK"));
    assert_eq!(json_lines(&out).len(), 1);
}

// ------------------------------------------------------------------ batch

#[test]
fn several_inputs_go_to_a_directory_and_a_failure_does_not_stop_the_rest() {
    let fx = Fixture::new();
    let out_dir = fx.dir.path().join("out");
    std::fs::create_dir(&out_dir).unwrap();
    let good1 = write(fx.dir.path(), "one.md", "# One\n");
    let bad = write(fx.dir.path(), "two.md", "$$\\left( x$$\n");
    let good2 = write(fx.dir.path(), "three.md", "# Three\n");
    let out = run(&[p(&good1), p(&bad), p(&good2), "-o", p(&out_dir)]);
    assert_eq!(out.status.code(), Some(4), "the first failure's status: {}", err(&out));
    assert!(out_dir.join("one.docx").exists() && out_dir.join("three.docx").exists());
    assert!(!out_dir.join("two.docx").exists());
    assert!(err(&out).contains("two.md"), "failures name their input: {}", err(&out));

    let json = run(&["--format", "json", p(&good1), p(&bad), "-o", p(&out_dir)]);
    assert_eq!(json_lines(&json).len(), 2);
}

#[test]
fn several_inputs_without_dash_o_write_next_to_each_input() {
    let fx = Fixture::new();
    let a = write(fx.dir.path(), "a.md", "a\n");
    let b = write(fx.dir.path(), "b.md", "b\n");
    assert_eq!(run(&[p(&a), p(&b)]).status.code(), Some(0));
    assert!(fx.dir.path().join("a.docx").exists() && fx.dir.path().join("b.docx").exists());
}

#[test]
fn two_inputs_with_one_output_name_are_refused_before_anything_is_written() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.dir.path().join("x")).unwrap();
    std::fs::create_dir_all(fx.dir.path().join("y")).unwrap();
    std::fs::create_dir_all(fx.dir.path().join("out")).unwrap();
    let a = write(&fx.dir.path().join("x"), "same.md", "a\n");
    let b = write(&fx.dir.path().join("y"), "same.md", "b\n");
    let out = run(&[p(&a), p(&b), "-o", p(&fx.dir.path().join("out"))]);
    assert_eq!(out.status.code(), Some(2), "{}", err(&out));
    assert_eq!(std::fs::read_dir(fx.dir.path().join("out")).unwrap().count(), 0);
}

// ------------------------------------------------------------------ environment

#[test]
fn environment_variables_set_operational_options() {
    let fx = Fixture::new();
    let lossy = write(fx.dir.path(), "lossy.md", LOSSY);
    let strict = cmd(&[p(&lossy)]).env("MD2DOCX_STRICT", "1").output().unwrap();
    assert_eq!(strict.status.code(), Some(5), "{}", err(&strict));
    let json = cmd(&[p(&lossy)]).env("MD2DOCX_FORMAT", "json").output().unwrap();
    assert!(err(&json).starts_with("{\"schema\":1"));
    // A container can tighten the policy but a limit still cannot be raised past --untrusted's.
    let raised = cmd(&[p(&lossy)])
        .env("MD2DOCX_UNTRUSTED", "1")
        .env("MD2DOCX_MAX_INPUT_BYTES", "1G")
        .output()
        .unwrap();
    assert_eq!(raised.status.code(), Some(2), "{}", err(&raised));
    let lowered = cmd(&[p(&lossy)])
        .env("MD2DOCX_UNTRUSTED", "1")
        .env("MD2DOCX_MAX_INPUT_BYTES", "1M")
        .output()
        .unwrap();
    assert_eq!(lowered.status.code(), Some(0), "{}", err(&lowered));
}

#[test]
fn source_date_epoch_makes_the_output_reproducible() {
    let fx = Fixture::new();
    let a = write(fx.dir.path(), "a.md", "# Same\n\nbytes\n");
    let first = cmd(&["-q", p(&a), "-o", "-"])
        .env("SOURCE_DATE_EPOCH", "1700000000")
        .output()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = cmd(&["-q", p(&a), "-o", "-"])
        .env("SOURCE_DATE_EPOCH", "1700000000")
        .output()
        .unwrap();
    assert_eq!(first.stdout, second.stdout);
    assert!(zip_text(&first.stdout, "docProps/core.xml").contains("2023-11-14T22:13:20Z"));
    let bad = cmd(&["-q", p(&a), "-o", "-"])
        .env("SOURCE_DATE_EPOCH", "soon")
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
}

// ------------------------------------------------------------------ document flags

#[test]
fn document_flags_reach_the_file() {
    let fx = Fixture::new();
    let a = write(fx.dir.path(), "a.md", "# T\n\ntext\n");
    let out = run(&[
        "-q",
        p(&a),
        "-o",
        "-",
        "--title",
        "My Title",
        "--author",
        "Me",
        "--page-size",
        "letter",
        "--landscape",
        "--margin",
        "1in",
        "--body-font",
        "Georgia",
        "--page-numbers",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    let core = zip_text(&out.stdout, "docProps/core.xml");
    assert!(core.contains("My Title") && core.contains("Me"), "{core}");
    let doc = zip_text(&out.stdout, "word/document.xml");
    assert!(
        doc.contains("w:w=\"15840\"") && doc.contains("w:h=\"12240\""),
        "landscape letter"
    );
    assert!(zip_text(&out.stdout, "word/styles.xml").contains("Georgia"));
    assert!(zip_entries(&out.stdout).iter().any(|e| e.starts_with("word/footer")));
}

#[test]
fn link_flags_narrow_what_a_link_may_do() {
    let fx = Fixture::new();
    let a = write(
        fx.dir.path(),
        "a.md",
        "[m](mailto:me@example.com) [h](https://example.com) <a href=\"https://html.example/\">raw</a>\n",
    );
    let schemes = run(&["-q", p(&a), "-o", "-", "--link-schemes", "https"]);
    let rels = zip_text(&schemes.stdout, "word/_rels/document.xml.rels");
    assert!(
        rels.contains("https://example.com") && !rels.contains("mailto:"),
        "{rels}"
    );
    let no_html = run(&["-q", p(&a), "-o", "-", "--no-html-links"]);
    let rels = zip_text(&no_html.stdout, "word/_rels/document.xml.rels");
    assert!(!rels.contains("html.example"), "{rels}");
}

// ------------------------------------------------------------------ support text

#[test]
fn completions_man_and_version_describe_the_tool() {
    let comp = run(&["--completions", "bash"]);
    assert_eq!(comp.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&comp.stdout).contains("md2docx-cli"));
    let man = run(&["--man"]);
    assert_eq!(man.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&man.stdout).contains(".TH"));
    let version = run(&["--version"]);
    assert!(
        String::from_utf8_lossy(&version.stdout).contains("features:"),
        "{:?}",
        version
    );
}
