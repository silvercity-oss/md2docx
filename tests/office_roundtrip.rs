//! The fixtures survive a real word processor: LibreOffice opens each one, saves it again as
//! `.docx`, and the result keeps the content, the pictures with their alt text, the equations and
//! the footnotes. Skipped when `soffice` is not installed, unless `REQUIRE_SOFFICE` is set (CI).
//!
//! This is the automated half of the "open, edit, save, reopen" check. Editing by hand in Microsoft
//! Word is a manual step: see `docs/integration-test-plan.md`.
#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use common::fixtures;
use common::*;
use silvercity_oss::{convert, LossPolicy};

fn soffice() -> Option<PathBuf> {
    ["soffice", "libreoffice"].iter().find(|n| have(n)).map(PathBuf::from)
}

/// Convert `input` to `format` with LibreOffice into `outdir`, in a throwaway profile.
fn lo_convert(bin: &Path, profile: &Path, input: &Path, format: &str, outdir: &Path) -> PathBuf {
    let started = Instant::now();
    let out = Command::new(bin)
        .arg(format!("-env:UserInstallation=file://{}", profile.display()))
        .args([
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--convert-to",
            format,
            "--outdir",
        ])
        .arg(outdir)
        .arg(input)
        .output()
        .expect("soffice starts");
    assert!(
        out.status.success(),
        "soffice failed after {:?}: {}",
        started.elapsed(),
        String::from_utf8_lossy(&out.stderr)
    );
    let ext = format.split(':').next().unwrap();
    let produced = outdir.join(input.file_stem().unwrap()).with_extension(ext);
    assert!(
        produced.exists(),
        "LibreOffice produced nothing for {}",
        input.display()
    );
    produced
}

#[test]
fn libreoffice_opens_saves_and_reopens_every_fixture() {
    let Some(bin) = soffice() else {
        assert!(
            std::env::var_os("REQUIRE_SOFFICE").is_none(),
            "REQUIRE_SOFFICE is set but soffice is missing"
        );
        eprintln!("soffice not installed; skipping");
        return;
    };
    let work = tempfile::tempdir().unwrap();
    let profile = work.path().join("profile");
    let cfg = fixtures::config();
    let deadline = Instant::now() + Duration::from_secs(600);

    for (stem, markdown, strict_ok) in fixtures::SOURCES {
        assert!(Instant::now() < deadline, "the round trip is taking too long");
        let policy = if *strict_ok {
            LossPolicy::Reject
        } else {
            LossPolicy::Allow
        };
        let original =
            convert(markdown, &cfg.clone().with_loss_policy(policy)).unwrap_or_else(|e| panic!("{stem}: {e}"));
        let src_dir = work.path().join("src");
        let out_dir = work.path().join("saved");
        let again_dir = work.path().join("text");
        for d in [&src_dir, &out_dir, &again_dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        let input = src_dir.join(format!("{stem}.docx"));
        std::fs::write(&input, &original.docx).unwrap();

        // Open + save.
        let saved = lo_convert(&bin, &profile, &input, "docx:MS Word 2007 XML", &out_dir);
        let saved_bytes = std::fs::read(&saved).unwrap();
        let document = zip_text(&saved_bytes, "word/document.xml");
        assert_well_formed(&document);

        // Reopen the saved file and read its text.
        let text_file = lo_convert(&bin, &profile, &saved, "txt:Text", &again_dir);
        let text = std::fs::read_to_string(&text_file).unwrap();

        match *stem {
            "01-structure" => {
                for needle in [
                    "Heading one",
                    "Second step",
                    "nested bullet",
                    "Bolt",
                    "low",
                    "Closing paragraph.",
                ] {
                    assert!(
                        text.contains(needle),
                        "{stem}: `{needle}` lost in the round trip:\n{text}"
                    );
                }
                assert!(document.contains("<w:tbl>"), "{stem}: the table did not survive");
                assert!(
                    document.contains("w:hyperlink")
                        || zip_text(&saved_bytes, "word/_rels/document.xml.rels").contains("https://example.com/page")
                );
            }
            "02-math" => {
                assert!(
                    document.contains("<m:oMath"),
                    "{stem}: equations were flattened: {}",
                    &document[..document.len().min(400)]
                );
                assert!(text.contains("First") && text.contains("Third"));
            }
            "03-images" => {
                assert_eq!(document.matches("<w:drawing>").count(), 3, "{stem}: pictures lost");
                assert!(
                    document.contains("Architecture overview showing three services"),
                    "{stem}: alt text lost in the round trip"
                );
            }
            "04-footnotes" => {
                assert!(zip_entries(&saved_bytes).iter().any(|n| n == "word/footnotes.xml"));
                let notes = zip_text(&saved_bytes, "word/footnotes.xml");
                assert!(
                    notes.contains("The first note") && notes.contains("The second note"),
                    "{notes}"
                );
            }
            "05-diagram" => {
                assert_eq!(document.matches("<w:drawing>").count(), 1);
                assert!(
                    document.contains("Login flow: the user submits credentials"),
                    "{stem}: the diagram's description was lost"
                );
            }
            _ => {
                assert!(text.contains("relative link") && text.contains("remote photo"));
            }
        }
        eprintln!(
            "{stem}: ok ({} bytes -> {} bytes)",
            original.docx.len(),
            saved_bytes.len()
        );
    }
}
