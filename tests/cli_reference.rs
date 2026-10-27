//! `--extract-reference` / `--reference-style`, and the modes that print facts as JSON
//! (`--capabilities`, `--list-codes`, `--print-config`).

#![cfg(feature = "cli")]

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::*;
use serde_json::Value;

fn run(args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_md2docx-cli"));
    c.args(args);
    for v in [
        "MD2DOCX_UNTRUSTED",
        "MD2DOCX_STRICT",
        "MD2DOCX_FORMAT",
        "MD2DOCX_QUIET",
        "SOURCE_DATE_EPOCH",
    ] {
        c.env_remove(v);
    }
    c.output().expect("md2docx-cli should start")
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn json_of(o: &Output) -> Value {
    assert!(o.status.success(), "{}", err(o));
    serde_json::from_slice(&o.stdout).expect("standard output is JSON")
}

/// A brand `.docx`: Georgia 12 pt, made by the converter itself.
fn brand(fx: &Fixture) -> std::path::PathBuf {
    let md = fx.path("seed.md");
    std::fs::write(&md, "# Seed\n").unwrap();
    let docx = fx.path("brand.docx");
    let o = run(&[p(&md), "--body-font", "Georgia", "--body-size", "12", "-o", p(&docx)]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    docx
}

#[test]
fn extracting_a_reference_writes_json_next_to_it_that_reproduces_the_look() {
    let fx = Fixture::new();
    let docx = brand(&fx);
    let o = run(&["--extract-reference", p(&docx)]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    let json_path = fx.path("brand.json");
    assert!(err(&o).contains("brand.json"), "{}", err(&o));
    let snapshot: Value = serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(snapshot["schema"], 1);
    assert_eq!(snapshot["source"], "brand.docx");
    assert_eq!(snapshot["doc_defaults"]["font"], "Georgia");
    assert_eq!(snapshot["doc_defaults"]["size_pt"], 12.0);
    let ids: Vec<&str> = snapshot["styles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for want in ["Normal", "Heading1", "InlineCode", "CodeBlock", "Hyperlink"] {
        assert!(ids.contains(&want), "{want} in {ids:?}");
    }

    // Converting with the snapshot gives the same styles as converting with the .docx.
    let md = fx.path("doc.md");
    std::fs::write(&md, "# Hello\n\nText `code`.\n").unwrap();
    let with_doc = run(&[p(&md), "--reference-doc", p(&docx), "-o", "-"]);
    let with_json = run(&[p(&md), "--reference-style", p(&json_path), "-o", "-"]);
    assert_eq!(with_doc.status.code(), Some(0), "{}", err(&with_doc));
    assert_eq!(with_json.status.code(), Some(0), "{}", err(&with_json));
    assert_eq!(
        zip_text(&with_doc.stdout, "word/styles.xml"),
        zip_text(&with_json.stdout, "word/styles.xml")
    );
    assert!(zip_text(&with_json.stdout, "word/styles.xml").contains("Georgia"));
}

#[test]
fn an_edited_snapshot_changes_the_output() {
    let fx = Fixture::new();
    let docx = brand(&fx);
    let o = run(&["--extract-reference", p(&docx), "-o", "-"]);
    let text = String::from_utf8(o.stdout).unwrap();
    // Edit the XML of the snapshot: a different heading colour.
    assert!(text.contains("1F3864"));
    let edited = text.replace("1F3864", "AA0000");
    let json_path = fx.path("edited.json");
    std::fs::write(&json_path, edited).unwrap();
    let md = fx.path("doc.md");
    std::fs::write(&md, "# Hello\n").unwrap();
    let out = run(&[p(&md), "--reference-style", p(&json_path), "-o", "-"]);
    assert_eq!(out.status.code(), Some(0), "{}", err(&out));
    assert!(zip_text(&out.stdout, "word/styles.xml").contains("AA0000"));
}

#[test]
fn the_extract_output_path_and_stdout_are_honoured_and_a_clash_is_refused() {
    let fx = Fixture::new();
    let docx = brand(&fx);
    let target = fx.path("custom-name.json");
    let o = run(&["--extract-reference", p(&docx), "-o", p(&target), "-q"]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert!(err(&o).is_empty(), "-q is quiet: {}", err(&o));
    assert!(target.exists());
    let clash = run(&["--extract-reference", p(&docx), "-o", p(&docx)]);
    assert_eq!(clash.status.code(), Some(2), "{}", err(&clash));
    let missing = run(&["--extract-reference", p(&fx.path("nope.docx"))]);
    assert_eq!(missing.status.code(), Some(2), "{}", err(&missing));
}

#[test]
fn the_reference_options_cannot_be_combined_and_bad_files_are_usage_errors() {
    let fx = Fixture::new();
    let docx = brand(&fx);
    let md = fx.path("doc.md");
    std::fs::write(&md, "# Hello\n").unwrap();
    let json_path = fx.path("s.json");
    std::fs::write(&json_path, "{}").unwrap();
    for args in [
        vec![p(&md), "--reference-doc", p(&docx), "--reference-style", p(&json_path)],
        vec![p(&md), "--extract-reference", p(&docx)],
    ] {
        assert_eq!(run(&args).status.code(), Some(2), "{args:?}");
    }
    let bad = run(&[p(&md), "--reference-style", p(&json_path)]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(
        err(&bad).contains("reference style") && err(&bad).contains("schema"),
        "{}",
        err(&bad)
    );
}

#[test]
fn list_codes_lists_every_code() {
    let v = json_of(&run(&["--list-codes"]));
    let codes = v["codes"].as_array().unwrap();
    assert!(codes.len() >= 20);
    assert!(codes.iter().all(|c| c["code"].is_string() && c["summary"].is_string()));
    assert!(codes.iter().any(|c| c["code"] == "link.dropped"));
}

#[test]
fn capabilities_describe_the_build_the_presets_and_the_tools() {
    let v = json_of(&run(&["--capabilities"]));
    assert_eq!(v["name"], "md2docx-cli");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["exit_codes"].as_array().unwrap().len(), 8);
    assert!(v["options"]["highlight_languages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l == "rust"));
    assert!(
        v["presets"]["untrusted"]["limits"]["max_input_bytes"].as_u64()
            < v["presets"]["trusted"]["limits"]["max_input_bytes"].as_u64()
    );
    for tool in ["latex", "dvipng", "merman-cli", "mmdc"] {
        assert!(v["external_tools"][tool]["found"].is_boolean(), "{tool}");
    }
}

#[test]
fn print_config_shows_what_the_options_resolve_to() {
    let trusted = json_of(&run(&[
        "--print-config",
        "--body-font",
        "Georgia",
        "--max-input-bytes",
        "1M",
    ]));
    assert_eq!(trusted["document"]["body_font"], "Georgia");
    assert_eq!(trusted["limits"]["max_input_bytes"], 1_048_576);
    assert_eq!(trusted["image_access"], "BaseDir");
    let untrusted = json_of(&run(&["--print-config", "--untrusted"]));
    assert_eq!(untrusted["image_access"], "None");
    assert_eq!(untrusted["diagrams"]["enabled"], false);
    // The same validation as a conversion: an untrusted limit cannot be raised.
    let raised = run(&["--print-config", "--untrusted", "--max-input-bytes", "999M"]);
    assert_eq!(raised.status.code(), Some(2), "{}", err(&raised));
}

#[test]
fn the_json_report_diagnostics_match_the_library_serialisation() {
    // `Report::to_json` and `--format json` describe a finding with the same fields.
    let fx = Fixture::new();
    let md = fx.path("doc.md");
    std::fs::write(&md, "[x](javascript:alert(1))\n").unwrap();
    let cli = run(&[p(&md), "--check", "--format", "json"]);
    let cli: Value = serde_json::from_str(err(&cli).lines().next().unwrap()).unwrap();
    let lib = md2docx::analyze("[x](javascript:alert(1))\n", &md2docx::Config::trusted()).unwrap();
    let lib: Value = serde_json::from_str(&lib.to_json()).unwrap();
    assert_eq!(cli["diagnostics"], lib["diagnostics"]);
    assert_eq!(cli["stats"], lib["stats"]);
}

/// `docs/report.schema.json` names exactly the fields the report has (checked here without a
/// schema validator: the required keys, the diagnostic keys and the stats keys).
#[test]
fn the_published_schema_matches_the_report() {
    let schema: Value = serde_json::from_str(include_str!("../docs/report.schema.json")).unwrap();
    let keys = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect();
        k.sort();
        k
    };
    let fx = Fixture::new();
    let md = fx.path("doc.md");
    std::fs::write(&md, "# H\n\n[x](javascript:alert(1))\n").unwrap();
    let o = run(&[p(&md), "--check", "--format", "json"]);
    let report: Value = serde_json::from_str(err(&o).lines().next().unwrap()).unwrap();
    let mut have: Vec<String> = report.as_object().unwrap().keys().cloned().collect();
    have.sort();
    assert_eq!(have, keys(&schema["required"]));
    let diagnostic = &report["diagnostics"][0];
    let mut have: Vec<String> = diagnostic.as_object().unwrap().keys().cloned().collect();
    have.sort();
    assert_eq!(have, keys(&schema["$defs"]["diagnostic"]["required"]));
    let mut have: Vec<String> = report["stats"].as_object().unwrap().keys().cloned().collect();
    have.sort();
    assert_eq!(have, keys(&schema["properties"]["stats"]["oneOf"][1]["required"]));
}

/// The defaults `CLI.md` and `--help` quote are the ones `--print-config` reports.
#[test]
fn the_documented_default_limits_are_the_real_ones() {
    let trusted = json_of(&run(&["--print-config"]));
    let untrusted = json_of(&run(&["--print-config", "--untrusted"]));
    let get = |v: &Value, k: &str| v["limits"][k].as_u64().unwrap();
    let mib = 1024 * 1024;
    assert_eq!(get(&trusted, "max_input_bytes"), 10 * mib);
    assert_eq!(get(&untrusted, "max_input_bytes"), 2 * mib);
    assert_eq!(get(&trusted, "max_output_bytes"), 256 * mib);
    assert_eq!(get(&untrusted, "max_output_bytes"), 50 * mib);
    assert_eq!(get(&trusted, "max_uncompressed_bytes"), 1024 * mib);
    assert_eq!(get(&untrusted, "max_uncompressed_bytes"), 64 * mib);
    assert_eq!(get(&trusted, "max_elements"), 500_000);
    assert_eq!(get(&untrusted, "max_elements"), 60_000);
    assert_eq!(get(&trusted, "max_image_bytes"), 25 * mib);
    assert_eq!(get(&untrusted, "max_image_bytes"), 5 * mib);
    assert_eq!(get(&trusted, "max_distinct_images"), 1000);
    assert_eq!(get(&untrusted, "max_distinct_images"), 100);
    assert_eq!(untrusted["limits"]["max_conversion_time_secs"], 120.0);
    assert!(trusted["limits"]["max_conversion_time_secs"].is_null());
}
