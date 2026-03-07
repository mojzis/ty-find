/// Tests for multi-workspace daemon scenarios.
///
/// Verifies that the daemon correctly handles requests for different workspaces
/// in sequence — the bug being that after loading workspace A, requests for
/// workspace B would fail with "Failed to resolve path" because file paths
/// were resolved relative to the daemon's CWD instead of the workspace root.
///
/// All sub-cases run inside a single `#[tokio::test]` to serialise daemon
/// access (the daemon socket is shared across tests).
#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_project")
}

fn test_project2_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_project2")
}

/// Run tyf with --workspace pointing at the given root.
fn run_tyf(workspace: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("failed to run tyf")
}

#[tokio::test]
async fn test_cross_workspace_find_and_inspect() {
    common::require_ty();

    // ── 1. find in test_project (workspace A) ───────────────────────
    let out = run_tyf(&test_project_root(), &["find", "Animal"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "find Animal in test_project should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        predicate::str::contains("models.py").eval(&stdout),
        "expected Animal in models.py, got:\n{stdout}"
    );

    // ── 2. find in test_project2 (workspace B) AFTER workspace A ────
    let out = run_tyf(&test_project2_root(), &["find", "UserService"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "find UserService in test_project2 should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        predicate::str::contains("services.py").eval(&stdout),
        "expected UserService in services.py, got:\n{stdout}"
    );

    // ── 3. inspect in test_project2 (exercises file path resolution) ─
    let out = run_tyf(&test_project2_root(), &["inspect", "User"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "inspect User in test_project2 should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        predicate::str::contains("services.py").eval(&stdout),
        "expected User defined in services.py, got:\n{stdout}"
    );

    // ── 4. back to test_project (workspace A still works) ───────────
    let out = run_tyf(&test_project_root(), &["find", "create_dog"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "find create_dog in test_project should still work, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        predicate::str::contains("models.py").eval(&stdout),
        "expected create_dog in models.py, got:\n{stdout}"
    );

    // ── 5. inspect with --file in test_project2 (daemon path resolution) ─
    let file_path = test_project2_root().join("services.py");
    let out = run_tyf(
        &test_project2_root(),
        &["inspect", "--file", &file_path.to_string_lossy(), "UserService"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "inspect --file services.py UserService should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        predicate::str::contains("UserService").eval(&stdout),
        "expected UserService in output, got:\n{stdout}"
    );

    // ── 6. daemon status should show workspace info ─────────────────
    let out = {
        let mut cmd = cargo_bin_cmd!("tyf");
        cmd.arg("daemon").arg("status");
        cmd.output().expect("failed to run tyf daemon status")
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        predicate::str::contains("running").eval(&stdout),
        "daemon should be running, got:\n{stdout}"
    );
    // After hitting two workspaces, status should list loaded workspace paths
    assert!(
        predicate::str::contains("test_project").eval(&stdout),
        "daemon status should list loaded workspaces, got:\n{stdout}"
    );
}
