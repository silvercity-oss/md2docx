//! The optional in-process Mermaid renderer (cargo feature `mermaid`).
#![cfg(feature = "mermaid")]

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use md2docx::mermaid::{InProcessMermaid, MermaidProfile};
use md2docx::{
    convert, markdown_to_docx, CancelToken, Code, CommandSpec, Config, CrateError, DiagramFailure, FenceError,
    FenceRenderer, LossPolicy, RenderContext,
};

const FLOW: &str = "```mermaid\nflowchart LR\n  A[Start] --> B{Choice}\n  B -->|yes| C[Done]\n```\n";

fn chain(n: usize) -> String {
    let mut s = String::from("```mermaid\nflowchart TD\n");
    for i in 0..n {
        s.push_str(&format!("  N{i}[Node {i}] --> N{}[Node {}]\n", i + 1, i + 1));
    }
    s.push_str("```\n");
    s
}

#[test]
fn a_diagram_becomes_an_accessible_picture_without_starting_any_process() {
    // The configured external command does not exist: if anything tried to spawn it, this fails.
    let mut cfg = Config::default().with_in_process_mermaid();
    cfg.graphs.mermaid = CommandSpec::new("definitely-not-a-real-program", ["{input}", "{output}"]);
    let docx = markdown_to_docx(FLOW, &cfg).expect("renders in-process");
    assert_eq!(media_count(&docx), 1);
    let body = zip_text(&docx, "word/document.xml");
    assert!(body.contains("<w:drawing>"));
    assert!(
        body.contains(r#"descr="Diagram: flowchart LR A[Start] --&gt; B{Choice}"#),
        "{body}"
    );
    let png = zip_bytes(
        &docx,
        &zip_entries(&docx).into_iter().find(|n| n.ends_with(".png")).unwrap(),
    );
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}

/// Wraps a renderer and records what is on disk under the scratch root while it runs.
struct WatchesScratch {
    root: std::path::PathBuf,
    inner: InProcessMermaid,
    seen_during: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl FenceRenderer for WatchesScratch {
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError> {
        self.render_with(source, &RenderContext::unbounded())
    }
    fn render_with(&self, source: &str, ctx: &RenderContext) -> Result<Vec<u8>, FenceError> {
        let entries = std::fs::read_dir(&self.root).unwrap().count();
        self.seen_during.lock().unwrap().push(entries);
        self.inner.render_with(source, ctx)
    }
}

#[test]
fn nothing_is_written_to_the_scratch_directory() {
    let root = tempfile::tempdir().unwrap();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let cfg = Config::default().with_scratch_dir(root.path()).with_fence_renderer(
        "mermaid",
        WatchesScratch {
            root: root.path().to_path_buf(),
            inner: InProcessMermaid::new(),
            seen_during: Arc::clone(&seen),
        },
    );
    markdown_to_docx(FLOW, &cfg).unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec![0],
        "no scratch directory even existed during the render"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn bad_diagram_text_follows_the_configured_failure_policy() {
    let md = "```mermaid\nflowchart TD\n A --> \n[[[\n```\n";
    let cfg = Config::default().with_in_process_mermaid();
    match markdown_to_docx(md, &cfg) {
        Err(CrateError::ExternalToolFailed { program, detail }) => {
            assert!(
                program.contains("mermaid") && detail.contains("parse"),
                "{program}: {detail}"
            );
        }
        other => panic!("{:?}", other.map(|d| d.len())),
    }
    let mut keep = cfg.clone();
    keep.graphs.on_failure = DiagramFailure::KeepAsCode;
    let c = convert(md, &keep).unwrap();
    assert!(c.diagnostics.iter().any(|d| d.code == Code::DiagramKeptAsCode));
    assert!(matches!(
        markdown_to_docx(md, &keep.with_loss_policy(LossPolicy::Reject)),
        Err(CrateError::LossyContent { .. })
    ));
}

#[test]
fn a_diagram_over_merman_budget_is_a_limit_error_that_keep_as_code_does_not_hide() {
    let mut cfg = Config::default().with_fence_renderer(
        "mermaid",
        InProcessMermaid::new().with_profile(MermaidProfile::Constrained),
    );
    cfg.graphs.on_failure = DiagramFailure::KeepAsCode;
    let md = chain(400);
    let started = Instant::now();
    match markdown_to_docx(&md, &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("diagram"), "{m}"),
        other => panic!("{:?}", other.map(|d| d.len())),
    }
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn the_picture_is_scaled_to_fit_the_conversions_image_limits() {
    let mut cfg = Config::default().with_in_process_mermaid();
    cfg.limits.max_image_dimension = 300;
    cfg.limits.max_image_pixels = 60_000;
    let docx = markdown_to_docx(FLOW, &cfg).expect("fits after scaling down, not an error");
    let name = zip_entries(&docx).into_iter().find(|n| n.ends_with(".png")).unwrap();
    let img = image::load_from_memory(&zip_bytes(&docx, &name)).unwrap();
    assert!(
        img.width() <= 300 && img.height() <= 300,
        "{}x{}",
        img.width(),
        img.height()
    );
    assert!(u64::from(img.width()) * u64::from(img.height()) <= 60_000);
}

#[test]
fn the_conversion_deadline_interrupts_a_running_render() {
    let cfg = Config::default()
        .with_fence_renderer("mermaid", InProcessMermaid::new().with_profile(MermaidProfile::Trusted))
        .with_deadline(Duration::from_millis(150));
    let started = Instant::now();
    let r = markdown_to_docx(&chain(250), &cfg);
    assert!(matches!(r, Err(CrateError::TimedOut(_))), "{:?}", r.map(|d| d.len()));
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn cancelling_from_another_thread_interrupts_a_running_render() {
    let token = CancelToken::new();
    let cfg = Config::default()
        .with_fence_renderer("mermaid", InProcessMermaid::new().with_profile(MermaidProfile::Trusted))
        .with_cancel_token(token.clone());
    let canceller = {
        let token = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            token.cancel();
        })
    };
    let started = Instant::now();
    let r = markdown_to_docx(&chain(250), &cfg);
    canceller.join().unwrap();
    assert!(matches!(r, Err(CrateError::Cancelled(_))), "{:?}", r.map(|d| d.len()));
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn repeated_identical_diagrams_render_once() {
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    struct Counting(Arc<std::sync::atomic::AtomicUsize>, InProcessMermaid);
    impl FenceRenderer for Counting {
        fn render(&self, s: &str) -> Result<Vec<u8>, FenceError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.1.render(s)
        }
    }
    let cfg = Config::default().with_fence_renderer("mermaid", Counting(Arc::clone(&counter), InProcessMermaid::new()));
    markdown_to_docx(&format!("{FLOW}\n{FLOW}\n{FLOW}"), &cfg).unwrap();
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
}
