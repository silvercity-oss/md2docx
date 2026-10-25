//! Writes the representative `.docx` fixtures to a directory, for opening in Word, LibreOffice or
//! any other consumer by hand (open, edit, save, reopen).
//!
//! ```text
//! cargo run --example fixtures -- target/fixtures
//! ```
//!
//! The sources are `tests/fixtures/*.md`; images are generated in code and supplied through an
//! [`AssetMap`](silvercity_oss::AssetMap), the diagram is drawn (or rendered by merman with the
//! `mermaid` feature). The last fixture deliberately cannot be carried over faithfully: it is
//! written with the permissive policy, and its findings are printed.

#[path = "../tests/common/fixtures.rs"]
mod fixtures;

use silvercity_oss::{convert, LossPolicy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "target/fixtures".into());
    std::fs::create_dir_all(&dir)?;
    let cfg = fixtures::config();
    for (stem, markdown, strict_ok) in fixtures::SOURCES {
        let policy = if *strict_ok {
            LossPolicy::Reject
        } else {
            LossPolicy::Allow
        };
        let done = convert(markdown, &cfg.clone().with_loss_policy(policy))?;
        let path = std::path::Path::new(&dir).join(format!("{stem}.docx"));
        std::fs::write(&path, &done.docx)?;
        println!("{}  ({} bytes)", path.display(), done.docx.len());
        for d in &done.diagnostics {
            println!("    {d}");
        }
    }
    Ok(())
}
