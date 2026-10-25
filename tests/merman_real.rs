//! Tests against a real `merman-cli` (Rust Mermaid renderer). They skip when it
//! is not installed, unless `REQUIRE_MERMAN=1` (CI sets it so a broken install
//! fails loudly). Install with `cargo install merman-cli`.

mod common;

use std::io::Cursor;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use common::*;
use silvercity_oss::{markdown_to_docx, Config, CrateError};

fn merman_available() -> bool {
    let ok = have("merman-cli");
    if !ok {
        assert!(
            std::env::var("REQUIRE_MERMAN").map_or(true, |v| v.is_empty()),
            "REQUIRE_MERMAN is set but merman-cli is missing"
        );
        eprintln!("skipping: merman-cli not installed");
    }
    ok
}

fn fence(src: &str) -> String {
    format!("```mermaid\n{src}\n```\n")
}

#[test]
fn default_engine_renders_the_common_diagram_types() {
    if !merman_available() {
        return;
    }
    let diagrams = [
        "graph TD\n  A[Start] --> B{Ok?}\n  B -- Yes --> C[Done]\n  B -- No --> A",
        "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi",
        "classDiagram\n  Animal <|-- Duck\n  Animal : +int age\n  Duck : +swim()",
        "gantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  section A\n  Task one :a1, 2026-01-01, 10d",
        "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85",
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places",
        "stateDiagram-v2\n  [*] --> Still\n  Still --> Moving\n  Moving --> [*]",
    ];
    let md: String = diagrams.iter().map(|d| fence(d) + "\n").collect();
    let docx = markdown_to_docx(&md, &Config::native_math()).expect("every diagram type should render");
    assert_eq!(media_count(&docx), diagrams.len(), "one distinct picture per diagram");
    let body = zip_text(&docx, "word/document.xml");
    assert_well_formed(&body);
    assert_eq!(body.matches("<w:drawing>").count(), diagrams.len());
}

#[test]
fn bad_diagrams_are_clean_errors() {
    if !merman_available() {
        return;
    }
    match markdown_to_docx(
        &fence("graph TD\n  A --> \n  -->>> nonsense [[["),
        &Config::native_math(),
    ) {
        Err(CrateError::ExternalToolFailed { program, detail }) => {
            assert_eq!(program, "merman-cli");
            assert!(detail.to_lowercase().contains("parse error"), "{detail}");
        }
        other => panic!("expected a parse error, got {:?}", other.map(|d| d.len())),
    }
}

#[test]
fn absurdly_large_diagrams_are_refused_quickly() {
    if !merman_available() {
        return;
    }
    // merman enforces its own layout-work ceiling; we only need it to fail fast, not hang.
    let mut src = String::from("flowchart LR\n");
    for i in 0..1500 {
        src.push_str(&format!("  N{i}[Node {i}] --> N{}[Node {}]\n", i + 1, i + 1));
    }
    let mut cfg = Config::native_math();
    cfg.limits.max_graph_source_bytes = 1 << 20;
    let started = Instant::now();
    match markdown_to_docx(&fence(&src), &cfg) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("limit") || detail.contains("exceeded"), "{detail}");
        }
        other => panic!("expected a resource-limit error, got {:?}", other.map(|d| d.len())),
    }
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "took {:?}",
        started.elapsed()
    );
}

/// Diagram text is hostile input. Labels may reference local images and remote URLs;
/// the renderer must neither embed the local file nor connect anywhere.
#[test]
fn hostile_diagram_cannot_read_local_files_or_use_the_network() {
    if !merman_available() {
        return;
    }
    let fx = Fixture::new();
    // A magenta 100x100 "secret" nothing else in the output is allowed to contain.
    let secret = fx.path("secret.png");
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 255, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&secret, png.into_inner()).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let file_url = format!("file://{}", secret.display());

    let src = format!(
        "flowchart TD\n  A[\"<img src='{file_url}' width='100'>\"] --> B[\"<img src='http://127.0.0.1:{port}/beacon.png'>\"]\n  \
         C[\"<iframe src='{file_url}'></iframe><script>alert(1)</script>\"]\n  click A \"javascript:alert(1)\"\n  click B call steal()\n"
    );
    let src2 = format!(
        "%%{{init: {{\"securityLevel\":\"loose\",\"themeCSS\":\"@import url('http://127.0.0.1:{port}/css'); .node{{background:url({file_url})}}\"}}}}%%\n\
         flowchart LR\n  A[\"![x]({file_url})\"] --> B[\"`![y](http://127.0.0.1:{port}/md.png)`\"]\n"
    );
    for (n, src) in [src, src2].iter().enumerate() {
        // Either a rendered picture or a clean refusal is acceptable; leaking is not.
        let outcome = markdown_to_docx(&fence(src), &Config::native_math());
        if let Ok(docx) = outcome {
            for name in zip_entries(&docx)
                .into_iter()
                .filter(|n| n.starts_with("word/media/") && !n.ends_with('/'))
            {
                let mut z = zip::ZipArchive::new(Cursor::new(&docx)).unwrap();
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut z.by_name(&name).unwrap(), &mut bytes).unwrap();
                let img = image::load_from_memory(&bytes).unwrap().to_rgba8();
                let magenta = img
                    .pixels()
                    .filter(|p| p[0] > 240 && p[1] < 20 && p[2] > 240 && p[3] > 200)
                    .count();
                assert_eq!(magenta, 0, "diagram {n}: the local secret image leaked into the output");
            }
        }
    }
    assert!(
        matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "the renderer connected to the network"
    );

    // Positive control: the listener really would have noticed.
    let _client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        listener.accept().is_ok(),
        "the listener cannot detect connections, so the check above proves nothing"
    );
}

#[test]
fn many_diagrams_stay_within_the_time_budget() {
    if !merman_available() {
        return;
    }
    let md: String = (0..12)
        .map(|i| fence(&format!("graph TD\n  A{i} --> B{i}")) + "\n")
        .collect();
    let started = Instant::now();
    let docx = markdown_to_docx(&md, &Config::native_math()).unwrap();
    assert_eq!(media_count(&docx), 12);
    eprintln!("12 diagrams in {:?}", started.elapsed());
    assert!(started.elapsed() < Duration::from_secs(120));
}
