//! A misbehaving external tool must not be able to hang us, fill the disk, flood
//! memory, or outlive the conversion. Uses shell scripts, so Unix only.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use silvercity_oss::{markdown_to_docx, CommandSpec, Config, CrateError};

const GRAPH: &str = "```mermaid\ngraph TD; A-->B\n```\n";

fn config_with(tool: String) -> Config {
    let mut cfg = Config::default();
    cfg.graphs.mermaid = CommandSpec::new(tool, ["{input}", "{output}"]);
    cfg
}

#[test]
fn console_flooding_tool_is_killed_quickly() {
    let fx = Fixture::new();
    let mut cfg = config_with(script(fx.dir.path(), "flood", "exec yes 'flooding the console'"));
    cfg.tool_timeout = Duration::from_secs(60);
    let t = Instant::now();
    match markdown_to_docx(GRAPH, &cfg) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("1 MiB of console output"), "{detail}")
        }
        other => panic!("expected the flood to be stopped, got {:?}", other.map(|d| d.len())),
    }
    assert!(t.elapsed() < Duration::from_secs(20), "took {:?}", t.elapsed());
}

#[test]
fn slow_disk_filler_is_killed_mid_write() {
    let fx = Fixture::new();
    // Would append 400 MB over ~8 s; the workspace cap is 4 MB, so it must be stopped early.
    let body = "i=0\nwhile [ $i -lt 400 ]; do\n  dd if=/dev/zero bs=1048576 count=1 2>/dev/null >> \"$2\"\n  i=$((i+1))\n  sleep 0.02\ndone";
    let mut cfg = config_with(script(fx.dir.path(), "slow-fill", body));
    cfg.limits.max_workspace_bytes = 4 * 1024 * 1024;
    let t = Instant::now();
    match markdown_to_docx(GRAPH, &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("bytes of files"), "{m}"),
        other => panic!("expected LimitExceeded, got {:?}", other.map(|d| d.len())),
    }
    assert!(
        t.elapsed() < Duration::from_secs(15),
        "the filler ran for {:?}; it should have been killed early",
        t.elapsed()
    );
}

/// A fast disk can take hundreds of MB inside one polling interval (this failed on a macOS
/// runner): the tool is gone before it can be killed, so the cap must also be checked after
/// it exits. Which check fires depends on timing; the cap must hold either way.
#[test]
fn fast_disk_filler_is_still_refused() {
    let fx = Fixture::new();
    let mut cfg = config_with(script(
        fx.dir.path(),
        "fast-fill",
        "dd if=/dev/zero of=\"$2\" bs=1M count=200 2>/dev/null",
    ));
    cfg.limits.max_workspace_bytes = 4 * 1024 * 1024;
    assert!(matches!(
        markdown_to_docx(GRAPH, &cfg),
        Err(CrateError::LimitExceeded(_))
    ));
}

#[test]
fn oversized_tool_output_is_refused() {
    let fx = Fixture::new();
    let png = fx.path("fixture.png");
    let mut cfg = config_with(script(fx.dir.path(), "big", &format!("cp '{}' \"$2\"", png.display())));
    cfg.limits.max_image_bytes = 100; // the fixture PNG is larger than this
    assert!(matches!(markdown_to_docx(GRAPH, &cfg), Err(CrateError::LimitExceeded(m)) if m.contains("image")));
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn timeout_kills_the_tool_and_everything_it_started() {
    let fx = Fixture::new();
    let pidfile = fx.path("child.pid");
    // Like `mmdc`, the tool starts a helper (think headless Chrome) and waits on it.
    let tool = script(
        fx.dir.path(),
        "spawner",
        &format!("sleep 300 &\necho $! > '{}'\nwait", pidfile.display()),
    );
    let mut cfg = config_with(tool);
    cfg.tool_timeout = Duration::from_millis(500);
    assert!(matches!(
        markdown_to_docx(GRAPH, &cfg),
        Err(CrateError::ExternalToolFailed { detail, .. }) if detail.contains("timed out")
    ));
    let pid = std::fs::read_to_string(&pidfile)
        .expect("helper started")
        .trim()
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let survived = alive(&pid);
    if survived {
        let _ = std::process::Command::new("kill").args(["-9", &pid]).status(); // don't leak from the test itself
    }
    assert!(!survived, "helper process {pid} outlived the timeout");
}

#[test]
fn failing_tool_does_not_leave_helpers_running() {
    let fx = Fixture::new();
    let pidfile = fx.path("child.pid");
    let tool = script(
        fx.dir.path(),
        "spawn-then-fail",
        &format!("sleep 300 &\necho $! > '{}'\necho boom >&2\nexit 7", pidfile.display()),
    );
    assert!(matches!(
        markdown_to_docx(GRAPH, &config_with(tool)),
        Err(CrateError::ExternalToolFailed { detail, .. }) if detail.contains("boom")
    ));
    let pid = std::fs::read_to_string(&pidfile).unwrap().trim().to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let survived = alive(&pid);
    if survived {
        let _ = std::process::Command::new("kill").args(["-9", &pid]).status();
    }
    assert!(!survived, "helper process {pid} outlived a failed tool");
}

#[test]
fn total_tool_time_is_budgeted_across_the_whole_document() {
    let fx = Fixture::new();
    let png = fx.path("fixture.png");
    // Each distinct diagram takes ~0.4 s; ten of them would take ~4 s.
    let mut cfg = config_with(script(
        fx.dir.path(),
        "slowish",
        &format!("sleep 0.4\ncp '{}' \"$2\"", png.display()),
    ));
    cfg.limits.max_total_tool_time = Duration::from_millis(1500);
    let md: String = (0..10)
        .map(|i| format!("```mermaid\ngraph TD; A{i}-->B\n```\n\n"))
        .collect();
    let t = Instant::now();
    match markdown_to_docx(&md, &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("total time budget"), "{m}"),
        other => panic!("expected the budget to run out, got {:?}", other.map(|d| d.len())),
    }
    assert!(t.elapsed() < Duration::from_secs(10), "stopped after {:?}", t.elapsed());

    // The same document fits in a generous budget.
    cfg.limits.max_total_tool_time = Duration::from_secs(60);
    assert!(markdown_to_docx(&md, &cfg).is_ok());
}

#[test]
fn successful_tool_does_not_leave_helpers_running_either() {
    let fx = Fixture::new();
    let pidfile = fx.path("ok.pid");
    let tool = script(
        fx.dir.path(),
        "spawn-then-succeed",
        &format!(
            "sleep 300 &\necho $! > '{}'\ncp '{}' \"$2\"",
            pidfile.display(),
            fx.path("fixture.png").display()
        ),
    );
    assert!(markdown_to_docx(GRAPH, &config_with(tool)).is_ok());
    let pid = std::fs::read_to_string(&pidfile).unwrap().trim().to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let survived = alive(&pid);
    if survived {
        let _ = std::process::Command::new("kill").args(["-9", &pid]).status();
    }
    assert!(!survived, "helper process {pid} outlived a successful tool");
}

#[test]
fn files_hidden_deep_in_subdirectories_count_against_the_workspace_cap() {
    let fx = Fixture::new();
    let tool = script(
        fx.dir.path(),
        "deep-writer",
        &format!(
            "mkdir -p a/b/c/d/e/f\nhead -c 20000 /dev/zero > a/b/c/d/e/f/blob\ncp '{}' \"$2\"",
            fx.path("fixture.png").display()
        ),
    );
    let mut cfg = config_with(tool);
    cfg.limits.max_workspace_bytes = 10_000;
    match markdown_to_docx(GRAPH, &cfg) {
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("bytes of files"), "{m}"),
        other => panic!("expected LimitExceeded, got {:?}", other.map(|d| d.len())),
    }
}

#[test]
fn an_output_replaced_by_a_symlink_is_refused() {
    let fx = Fixture::new();
    let tool = script(
        fx.dir.path(),
        "linker",
        &format!("ln -s '{}' \"$2\"", fx.path("fixture.png").display()),
    );
    match markdown_to_docx(GRAPH, &config_with(tool)) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("not a regular file"), "{detail}")
        }
        other => panic!("expected the symlink to be refused, got {:?}", other.map(|d| d.len())),
    }
}
