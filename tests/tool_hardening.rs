//! A misbehaving external tool must not be able to hang us, fill the disk, flood
//! memory, or outlive the conversion. Uses shell scripts, so Unix only.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use md2docx::{markdown_to_docx, CommandSpec, Config, CrateError};

const GRAPH: &str = "```mermaid\ngraph TD; A-->B\n```\n";

fn config_with(tool: String) -> Config {
    let mut cfg = Config::trusted();
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
        Err(CrateError::LimitExceeded(m)) => assert!(m.contains("time budget"), "{m}"),
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

// ---- cleanup on every post-spawn exit path

fn wait_dead(pid: &str, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let survived = alive(pid);
    if survived {
        let _ = std::process::Command::new("kill").args(["-9", pid]).status(); // never leak from a test
    }
    !survived
}

fn read_pid(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .expect("helper started")
        .trim()
        .to_string()
}

/// A tool that exits at once, leaves a helper behind, and has written more than the workspace
/// budget: the budget error must not skip the helper's termination.
#[test]
fn fast_exit_with_helper_and_over_budget_workspace_still_ends_the_helper() {
    let fx = Fixture::new();
    let pidfile = fx.path("helper.pid");
    let tool = script(
        fx.dir.path(),
        "leave-and-fill",
        &format!(
            "sleep 300 &\necho $! > '{}'\ndd if=/dev/zero of=\"$2\" bs=1M count=8 2>/dev/null\nexit 0",
            pidfile.display()
        ),
    );
    let mut cfg = config_with(tool);
    cfg.limits.max_workspace_bytes = 1024 * 1024;
    assert!(matches!(
        markdown_to_docx(GRAPH, &cfg),
        Err(CrateError::LimitExceeded(m)) if m.contains("bytes of files")
    ));
    let pid = read_pid(&pidfile);
    assert!(wait_dead(&pid, 10), "helper {pid} outlived the workspace-limit error");
}

/// The console cap is enforced after the tool has exited as well, not only while it runs.
#[test]
fn console_limit_is_rechecked_after_exit_and_helpers_still_die() {
    let fx = Fixture::new();
    let pidfile = fx.path("helper.pid");
    let tool = script(
        fx.dir.path(),
        "print-and-leave",
        &format!(
            "sleep 300 &\necho $! > '{}'\nhead -c 3000000 /dev/zero | tr '\\000' x\nexit 0",
            pidfile.display()
        ),
    );
    match markdown_to_docx(GRAPH, &config_with(tool)) {
        Err(CrateError::ExternalToolFailed { detail, .. }) => {
            assert!(detail.contains("console output"), "{detail}")
        }
        other => panic!(
            "expected the console flood to be refused, got {:?}",
            other.map(|d| d.len())
        ),
    }
    let pid = read_pid(&pidfile);
    assert!(wait_dead(&pid, 10), "helper {pid} outlived the console-limit error");
}

/// Everything at once: console flood, over-budget workspace, a helper, and a fast exit.
#[test]
fn combined_failures_all_clean_up() {
    let fx = Fixture::new();
    let pidfile = fx.path("helper.pid");
    let tool = script(
        fx.dir.path(),
        "everything",
        &format!(
            "sleep 300 &\necho $! > '{}'\nhead -c 3000000 /dev/zero | tr '\\000' x\ndd if=/dev/zero of=\"$2\" bs=1M count=8 2>/dev/null\nexit 3",
            pidfile.display()
        ),
    );
    let root = tempfile::tempdir().unwrap();
    let mut cfg = config_with(tool).with_scratch_dir(root.path());
    cfg.limits.max_workspace_bytes = 1024 * 1024;
    assert!(markdown_to_docx(GRAPH, &cfg).is_err());
    assert!(wait_dead(&read_pid(&pidfile), 10), "helper outlived the failure");
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        0,
        "scratch left behind"
    );
}

// ---- cancellation and the conversion deadline

#[test]
fn cancelling_stops_a_running_tool_and_its_helpers() {
    let fx = Fixture::new();
    let pidfile = fx.path("helper.pid");
    let tool = script(
        fx.dir.path(),
        "long-runner",
        &format!("sleep 300 &\necho $! > '{}'\nwait", pidfile.display()),
    );
    let token = md2docx::CancelToken::new();
    let mut cfg = config_with(tool).with_cancel_token(token.clone());
    cfg.tool_timeout = Duration::from_secs(120);
    let canceller = {
        let token = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            token.cancel();
        })
    };
    let t = Instant::now();
    let result = markdown_to_docx(GRAPH, &cfg);
    canceller.join().unwrap();
    assert!(
        matches!(result, Err(CrateError::Cancelled(_))),
        "{:?}",
        result.map(|d| d.len())
    );
    assert!(t.elapsed() < Duration::from_secs(10), "took {:?}", t.elapsed());
    assert!(wait_dead(&read_pid(&pidfile), 10), "helper outlived the cancellation");
}

#[test]
fn a_token_cancelled_before_the_start_does_no_work() {
    let token = md2docx::CancelToken::new();
    token.cancel();
    let cfg = Config::native_math().with_cancel_token(token);
    assert!(matches!(
        markdown_to_docx("# Hello", &cfg),
        Err(CrateError::Cancelled(_))
    ));
}

#[test]
fn the_conversion_deadline_stops_a_running_tool() {
    let fx = Fixture::new();
    let pidfile = fx.path("helper.pid");
    let tool = script(
        fx.dir.path(),
        "sleeper",
        &format!("sleep 300 &\necho $! > '{}'\nwait", pidfile.display()),
    );
    let mut cfg = config_with(tool).with_deadline(Duration::from_millis(700));
    cfg.tool_timeout = Duration::from_secs(120);
    let t = Instant::now();
    let result = markdown_to_docx(GRAPH, &cfg);
    assert!(
        matches!(result, Err(CrateError::TimedOut(_))),
        "{:?}",
        result.map(|d| d.len())
    );
    assert!(t.elapsed() < Duration::from_secs(10), "took {:?}", t.elapsed());
    assert!(wait_dead(&read_pid(&pidfile), 10), "helper outlived the deadline");
}

// ---- the host-owned scratch root

#[test]
fn every_temporary_artifact_lives_under_the_host_scratch_root() {
    let fx = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    let seen = fx.path("seen.txt");
    // Record where the tool runs and where it was told to read and write.
    let tool = script(
        fx.dir.path(),
        "recorder",
        &format!(
            "pwd -P > '{seen}'\necho \"$1\" >> '{seen}'\necho \"$2\" >> '{seen}'\nls -a >> '{seen}'\ncp '{png}' \"$2\"",
            seen = seen.display(),
            png = fx.path("fixture.png").display()
        ),
    );
    let cfg = config_with(tool).with_scratch_dir(root.path());
    markdown_to_docx(GRAPH, &cfg).expect("converts");
    let root_real = std::fs::canonicalize(root.path()).unwrap();
    let log = std::fs::read_to_string(&seen).unwrap();
    let mut lines = log.lines();
    let cwd = std::path::PathBuf::from(lines.next().unwrap());
    assert_eq!(
        cwd.parent().unwrap(),
        root_real,
        "tool ran outside the scratch root: {log}"
    );
    assert!(cwd.file_name().unwrap().to_string_lossy().starts_with("md2docx-"));
    for path in [lines.next().unwrap(), lines.next().unwrap()] {
        let canonical_dir = std::fs::canonicalize(std::path::Path::new(path).parent().unwrap_or(&cwd)).ok();
        assert!(
            path.starts_with(root.path().to_str().unwrap()) || canonical_dir.is_some_and(|d| d.starts_with(&root_real)),
            "{path} is outside {}",
            root.path().display()
        );
    }
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        0,
        "scratch not removed after success"
    );
}

#[test]
fn scratch_is_removed_after_errors_too() {
    let fx = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    for body in ["exit 9", "echo boom >&2; exit 1", "cp /dev/null \"$2\""] {
        let cfg = config_with(script(fx.dir.path(), "fails", body)).with_scratch_dir(root.path());
        assert!(markdown_to_docx(GRAPH, &cfg).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0, "after `{body}`");
    }
}

#[test]
fn a_missing_scratch_root_is_an_error_not_a_fallback_to_the_system_temp_dir() {
    let fx = Fixture::new();
    let cfg = config_with(script(fx.dir.path(), "never-run", "exit 0")).with_scratch_dir("/nonexistent/job-dir");
    assert!(matches!(markdown_to_docx(GRAPH, &cfg), Err(CrateError::Io { .. })));
}

#[test]
fn sweep_scratch_recovers_leftovers_of_a_killed_worker_and_nothing_else() {
    let root = tempfile::tempdir().unwrap();
    let ours = root.path().join("md2docx-abc123");
    std::fs::create_dir(&ours).unwrap();
    std::fs::write(ours.join(".md2docx-scratch"), b"").unwrap();
    std::fs::write(ours.join("graph_1.png"), b"leftover").unwrap();
    // Look alike, but not made by this crate: no marker.
    let foreign = root.path().join("md2docx-not-ours");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("keep.txt"), b"keep").unwrap();
    let other = root.path().join("unrelated");
    std::fs::create_dir(&other).unwrap();
    // A symlink with the right name pointing at a directory that must survive.
    let precious = tempfile::tempdir().unwrap();
    std::fs::write(precious.path().join(".md2docx-scratch"), b"").unwrap();
    std::os::unix::fs::symlink(precious.path(), root.path().join("md2docx-link")).unwrap();

    // Too young to touch.
    assert_eq!(
        md2docx::sweep_scratch(root.path(), Duration::from_secs(3600)).unwrap(),
        0
    );
    assert!(ours.exists());
    // Old enough.
    assert_eq!(md2docx::sweep_scratch(root.path(), Duration::ZERO).unwrap(), 1);
    assert!(!ours.exists());
    assert!(foreign.join("keep.txt").exists() && other.exists());
    assert!(
        precious.path().join(".md2docx-scratch").exists(),
        "a symlink must not be followed"
    );
}
