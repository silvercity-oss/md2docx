//! Windows counterpart of the process-group tests in `tool_hardening.rs`: a diagram tool that
//! starts a helper and is then timed out must take the helper down with it (the tool runs in a
//! Job Object that is closed with the guard).
#![cfg(windows)]

use std::time::{Duration, Instant};

use md2docx::{markdown_to_docx, CommandSpec, Config, CrateError};

const GRAPH: &str = "```mermaid\ngraph TD; A-->B\n```\n";

#[test]
fn timeout_kills_the_tool_and_everything_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker.txt");
    let tool = dir.path().join("spawner.cmd");
    // The helper appends a line to the marker file about once a second for ten minutes; the tool
    // itself just waits, so the timeout is what ends it.
    std::fs::write(
        &tool,
        format!(
            "@echo off\r\nstart /b \"\" cmd /c \"for /l %%i in (1,1,600) do (echo x>>{m} & ping -n 2 127.0.0.1 >nul)\"\r\nping -n 300 127.0.0.1 >nul\r\n",
            m = marker.display()
        ),
    )
    .unwrap();

    let mut cfg = Config::trusted();
    cfg.graphs.mermaid = CommandSpec::new(tool.display().to_string(), ["{input}", "{output}"]);
    cfg.tool_timeout = Duration::from_secs(2);
    assert!(matches!(
        markdown_to_docx(GRAPH, &cfg),
        Err(CrateError::ExternalToolFailed { detail, .. }) if detail.contains("timed out")
    ));

    // Give a surviving helper time to append, then check that it stopped.
    let len = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while len(&marker) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(1));
    let before = len(&marker);
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(
        before,
        len(&marker),
        "the helper kept writing after the tool was timed out"
    );
}
