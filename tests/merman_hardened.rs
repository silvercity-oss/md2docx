//! `Config::untrusted_with_diagrams()` against a real `merman-cli`: diagrams
//! render, and hostile diagram text cannot make the tool read local files or
//! use the network. Skipped when merman-cli is absent unless `REQUIRE_MERMAN=1`.
//!
//! The "nothing leaked" checks only mean something if the detector can see a
//! leak, so each one has a positive control.

mod common;

use std::io::{Cursor, Read};
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

fn is_marker(p: &image::Rgba<u8>) -> bool {
    p[0] > 240 && p[1] < 20 && p[2] > 240 && p[3] > 200
}

/// Magenta pixels in every picture embedded in the document.
fn marker_pixels(docx: &[u8]) -> usize {
    let mut total = 0;
    for name in zip_entries(docx)
        .into_iter()
        .filter(|n| n.starts_with("word/media/") && !n.ends_with('/'))
    {
        let mut z = zip::ZipArchive::new(Cursor::new(docx)).unwrap();
        let mut bytes = Vec::new();
        z.by_name(&name).unwrap().read_to_end(&mut bytes).unwrap();
        total += image::load_from_memory(&bytes)
            .unwrap()
            .to_rgba8()
            .pixels()
            .filter(|p| is_marker(p))
            .count();
    }
    total
}

fn magenta_png(path: &std::path::Path) {
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 255, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(path, png.into_inner()).unwrap();
}

#[test]
fn the_preset_renders_diagrams_and_still_confines_everything_else() {
    if !merman_available() {
        return;
    }
    let fx = Fixture::new();
    let cfg = Config::untrusted_with_diagrams().with_base_dir(fx.dir.path());
    let md = format!(
        "# Report\n\n{}\nMath stays native: $E=mc^2$\n\n```dolphin\nnode a -> node b\n```\n\n![local](fixture.png)\n\n\
         <script>alert(1)</script>\n\n[x](javascript:alert(1))\n",
        fence("graph TD\n  A[Start] --> B{Ok?}\n  B -- Yes --> C[Done]")
    );
    let docx = markdown_to_docx(&md, &cfg).expect("hardened diagrams should render");
    // Exactly one picture: the diagram. The local image is not embedded.
    assert_eq!(media_count(&docx), 1);
    let b = zip_text(&docx, "word/document.xml");
    assert_well_formed(&b);
    assert_eq!(b.matches("<w:drawing>").count(), 1);
    assert!(b.contains("<m:oMath>"), "native math");
    let text = visible_text(&b);
    assert!(text.contains("node a -> node b"), "dolphin stays code: {text}");
    assert!(text.contains("[local]") && !text.contains("alert"));
    assert!(!zip_text(&docx, "word/_rels/document.xml.rels").contains("javascript"));
}

#[test]
fn hostile_diagrams_cannot_read_local_files_or_use_the_network() {
    if !merman_available() {
        return;
    }
    let fx = Fixture::new();
    let secret = fx.path("secret.png");
    magenta_png(&secret);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let file = format!("file://{}", secret.display());
    let net = format!("http://127.0.0.1:{port}/beacon.png");
    let path = secret.display();

    let vectors: Vec<(&str, String)> = vec![
        ("html label images", format!("flowchart TD\n  A[\"<img src='{file}'>\"] --> B[\"<img src='{net}'>\"]")),
        ("image node shape (file)", format!("flowchart TD\n  A@{{ img: \"{file}\", label: \"x\", pos: \"b\", w: 100, h: 100, constraint: \"on\" }}")),
        ("image node shape (network)", format!("flowchart TD\n  A@{{ img: \"{net}\", label: \"x\", pos: \"b\", w: 100, h: 100, constraint: \"on\" }}")),
        ("image node shape (plain path)", format!("flowchart TD\n  A@{{ img: \"{path}\", label: \"x\", w: 100, h: 100 }}")),
        ("css url() in style/classDef", format!("flowchart TD\n  A --> B\n  style A fill:url({file})\n  classDef c background-image:url({net})\n  class B c")),
        ("init directive themeCSS", format!("%%{{init: {{\"securityLevel\":\"loose\",\"themeCSS\":\"@import url({net}); .node rect{{fill:url({file})}}\"}}}}%%\nflowchart TD\n  A --> B")),
        ("markdown string labels", format!("flowchart TD\n  A[\"`![x]({file})`\"] --> B[\"`![y]({net})`\"]")),
        ("script, iframe, click callbacks", format!("flowchart TD\n  A[\"<iframe src='{file}'></iframe><script>alert(1)</script>\"]\n  click A \"javascript:alert(1)\"\n  click A call steal()")),
        ("raw SVG as the diagram text", format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\"><image href=\"{file}\" width=\"120\" height=\"120\"/><image href=\"{net}\"/></svg>")),
    ];

    let cfg = Config::untrusted_with_diagrams().with_base_dir(fx.dir.path());
    let mut rendered = 0;
    for (name, src) in &vectors {
        // A clean refusal and a harmless picture are both acceptable; a leak is not.
        match markdown_to_docx(&fence(src), &cfg) {
            Ok(docx) => {
                rendered += 1;
                assert_eq!(
                    marker_pixels(&docx),
                    0,
                    "[{name}] the local secret image appeared in the output"
                );
            }
            Err(CrateError::ExternalToolFailed { .. }) => {}
            Err(other) => panic!("[{name}] unexpected error: {other}"),
        }
    }
    assert!(
        rendered >= 4,
        "only {rendered} vectors rendered; the test would prove little"
    );
    assert!(
        matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "merman connected to the network"
    );

    // Raw SVG must never be treated as SVG input (that is what `--input-kind mermaid` is for).
    match markdown_to_docx(&fence(&vectors[8].1), &cfg) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("No Mermaid diagram type detected"), "{detail}");
        }
        other => panic!(
            "raw SVG should be refused as a diagram, got {:?}",
            other.map(|d| d.len())
        ),
    }

    // Positive controls: the network listener notices a connection, and the marker
    // detector notices magenta pixels in an embedded picture.
    let _client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(listener.accept().is_ok(), "the listener cannot detect connections");
    let controlled = Config::native_math().with_base_dir(fx.dir.path());
    let docx = markdown_to_docx("![m](secret.png)", &controlled).unwrap();
    assert!(
        marker_pixels(&docx) >= 10_000,
        "the marker detector cannot see a magenta picture"
    );
}

#[test]
fn hardened_runs_stay_within_their_time_budget() {
    if !merman_available() {
        return;
    }
    let cfg = Config::untrusted_with_diagrams();
    let md: String = (0..5)
        .map(|i| fence(&format!("graph TD\n  A{i} --> B{i}")) + "\n")
        .collect();
    let started = Instant::now();
    let docx = markdown_to_docx(&md, &cfg).unwrap();
    assert_eq!(media_count(&docx), 5);
    assert!(started.elapsed() < Duration::from_secs(60), "{:?}", started.elapsed());

    // An absurdly large diagram is refused by merman itself, quickly.
    let mut big = String::from("flowchart LR\n");
    for i in 0..1500 {
        big.push_str(&format!("  N{i}[N{i}] --> N{}[N{}]\n", i + 1, i + 1));
    }
    let mut cfg = cfg;
    cfg.limits.max_graph_source_bytes = 1 << 20;
    let started = Instant::now();
    assert!(matches!(
        markdown_to_docx(&fence(&big), &cfg),
        Err(CrateError::ExternalToolFailed { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(30));
}
