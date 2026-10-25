//! Embedding the converter in a service that converts documents from other people.
//!
//! What this shows, in the order a request handler would do it:
//!
//! 1. A configuration that is safe for untrusted Markdown *and* strict: the default
//!    ([`Config::default`]) policy, the host's own image bytes ([`AssetMap`]), a job directory the
//!    host owns, a cancel token and a deadline, and [`LossPolicy::Reject`].
//! 2. [`analyze`]: the findings without building the archive, to compare with the host's own preview.
//! 3. [`convert`]: the document and its findings, or [`CrateError::LossyContent`] listing every
//!    construct that could not be carried over, with line and column in the submitted text.
//! 4. Recovery after a worker was killed from outside: [`sweep_scratch`].
//!
//! Run it with `cargo run --example embed` (add `--features mermaid` for in-process diagrams).
//! Nothing here depends on a particular application: replace `accepted_images` with whatever
//! store your images come from.

use std::path::Path;
use std::time::Duration;

use silvercity_oss::{
    analyze, convert, sweep_scratch, AssetMap, CancelToken, Config, CrateError, Diagnostic, LossPolicy, Severity,
};

/// The images this document is allowed to show, as exact bytes. The Markdown only names a key;
/// nothing is read from disk and nothing is fetched.
fn accepted_images() -> AssetMap {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(160, 90, image::Rgba([30, 90, 160, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encoding a PNG in memory");
    let mut assets = AssetMap::new();
    assets.insert("assets/architecture", png);
    assets
}

/// One conversion's configuration. Everything on it is chosen by the host, never by the document.
fn config_for_job(job_dir: &Path, cancel: CancelToken) -> Config {
    let mut cfg = Config::default() // the untrusted-input policy: native math, no TeX, no files, no <a>
        .with_image_resolver(accepted_images())
        .with_loss_policy(LossPolicy::Reject) // strict: a degraded document is an error
        .with_scratch_dir(job_dir) // every temporary artifact is under storage the host accounts for
        .with_cancel_token(cancel) // the host can stop the job ...
        .with_deadline(Duration::from_secs(30)); // ... and it stops itself after 30 s

    // Application limits; the defaults already suit untrusted input. Tighten what your policy needs.
    cfg.limits.max_input_bytes = 512 * 1024;
    cfg.limits.max_image_bytes = 1024 * 1024; // one source encoding
    cfg.limits.max_distinct_images = 50;
    cfg.limits.max_image_pixels = 4_000_000;
    cfg.limits.max_total_decode_pixels = 16_000_000; // actual decode work across the document
    cfg.limits.max_total_normalized_bytes = 8 * 1024 * 1024;
    cfg.limits.max_output_bytes = 16 * 1024 * 1024;

    // Diagrams without a child process (needs the `mermaid` cargo feature and Rust 1.95).
    #[cfg(feature = "mermaid")]
    {
        cfg = cfg.with_in_process_mermaid();
    }
    cfg
}

fn show(findings: &[Diagnostic]) {
    for d in findings {
        println!("  {d}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The host owns this directory (one per job). The converter makes its own subdirectory in it,
    // and removes that on every normal exit; `sweep_scratch` handles the abnormal ones.
    let jobs = tempfile::tempdir()?;
    let cancel = CancelToken::new();
    let cfg = config_for_job(jobs.path(), cancel.clone());

    let good =
        "# Findings\n\nThe system looks like this:\n\n![Architecture overview](assets/architecture \"Figure 1\")\n\n\
                Energy is $E = mc^2$.\n";
    let bad = "# Findings\n\n![Photo](https://example.com/photo.png)\n\nSee [the spec](docs/spec.md).\n\n\
               ```mermaid\ngraph TD; A-->B\n```\n";

    println!("== analyze (no archive is built)");
    let report = analyze(good, &cfg)?;
    println!(
        "  {} finding(s); {} heading(s), {} picture(s), {} equation(s)",
        report.diagnostics.len(),
        report.stats.headings,
        report.stats.pictures,
        report.stats.equations
    );

    println!("== convert, faithful document");
    let done = convert(good, &cfg)?;
    println!("  {} bytes, {} finding(s)", done.docx.len(), done.diagnostics.len());
    show(&done.diagnostics);

    println!("== convert, document that cannot be carried over faithfully");
    match convert(bad, &cfg) {
        Err(CrateError::LossyContent { diagnostics }) => {
            // Nothing was produced. Show the author exactly what and where.
            show(&diagnostics);
            // Info findings never reject; only warnings do.
            assert!(diagnostics.iter().any(|d| d.severity >= Severity::Warning));
        }
        other => panic!(
            "expected the strict policy to refuse this document, got {:?}",
            other.map(|c| c.docx.len())
        ),
    }

    // The same document is fine under the permissive policy, with the same findings reported.
    let lenient = cfg.clone().with_loss_policy(LossPolicy::Allow);
    let degraded = convert(bad, &lenient)?;
    println!(
        "== permissive policy: {} bytes, {} finding(s)",
        degraded.docx.len(),
        degraded.diagnostics.len()
    );

    // A worker that was killed from outside never ran its destructors: sweep what it left.
    let removed = sweep_scratch(jobs.path(), Duration::from_secs(3600))?;
    println!("== recovery: removed {removed} leftover scratch director(ies)");
    Ok(())
}
