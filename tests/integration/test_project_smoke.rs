/// Smoke tests that exercise the `test_project/` fixture end-to-end.
///
/// These tests exercise the workspace-symbol lookup path (no `--file` flag) —
/// the same path users typically hit from the CLI.  They verify that inspect
/// returns hover information and references, which historically broke because
/// workspace-symbol responses point at the declaration keyword (`class`/`def`)
/// rather than the symbol name.
///
/// All sub-cases live inside a single `#[tokio::test]` to avoid flaky failures
/// from concurrent daemon access (each test process talks to the shared daemon
/// socket, so parallel execution can cause race conditions).
#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::PathBuf;

/// Workspace root pointing at the `test_project` directory.
fn test_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("test_project")
}

/// Run tyf with the given arguments against `test_project` and return stdout.
fn run_tyf(args: &[&str]) -> String {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(test_project_root());
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "tyf failed: {stdout}");
    stdout
}

#[tokio::test]
async fn test_project_inspect_and_references() {
    common::require_ty();

    // ── 1. inspect class via workspace symbols (no --file) ──────────
    let out = run_tyf(&["inspect", "Animal"]);
    assert!(
        predicate::str::contains("models.py").eval(&out),
        "expected definition in models.py, got:\n{out}"
    );
    assert!(
        !predicate::str::contains("No hover information").eval(&out),
        "hover should be present for Animal class, got:\n{out}"
    );

    // ── 2. inspect function via workspace symbols ───────────────────
    let out = run_tyf(&["inspect", "create_dog"]);
    assert!(
        !predicate::str::contains("No hover information").eval(&out),
        "hover should be present for create_dog, got:\n{out}"
    );
    assert!(
        predicate::str::contains("create_dog").eval(&out),
        "hover should mention create_dog, got:\n{out}"
    );

    // ── 3. inspect with --references ────────────────────────────────
    let out = run_tyf(&["inspect", "Animal", "--references"]);
    assert!(
        !predicate::str::contains("No references found").eval(&out),
        "references should be present for Animal, got:\n{out}"
    );
    assert!(
        predicate::str::contains("main.py").eval(&out),
        "references should include main.py, got:\n{out}"
    );

    // ── 4. references by symbol name (class) ────────────────────────
    let out = run_tyf(&["refs", "Animal"]);
    assert!(
        !predicate::str::contains("No references found").eval(&out),
        "references should be present for Animal, got:\n{out}"
    );
    assert!(
        predicate::str::contains("models.py").eval(&out),
        "should reference models.py, got:\n{out}"
    );
    assert!(
        predicate::str::contains("main.py").eval(&out),
        "should reference main.py, got:\n{out}"
    );

    // ── 5. references by symbol name (function) ─────────────────────
    let out = run_tyf(&["refs", "create_dog"]);
    assert!(
        !predicate::str::contains("No references found").eval(&out),
        "references should be present for create_dog, got:\n{out}"
    );

    // ── 6. inspect with --file (file-based path, regression check) ──
    let models = test_project_root().join("models.py");
    let models_str = models.to_string_lossy().to_string();
    let out = run_tyf(&["inspect", "Animal", "--file", &models_str]);
    assert!(
        !predicate::str::contains("No hover information").eval(&out),
        "hover should be present when using --file, got:\n{out}"
    );

    // ── 7. members via workspace symbols (no --file) ────────────────
    let out = run_tyf(&["members", "Animal"]);
    assert!(
        predicate::str::contains("Animal").eval(&out),
        "members should show Animal class, got:\n{out}"
    );
    assert!(
        predicate::str::contains("speak").eval(&out),
        "members should show speak method, got:\n{out}"
    );

    // ── 8. members with --file ──────────────────────────────────────
    let out = run_tyf(&["members", "Dog", "--file", &models_str]);
    assert!(
        predicate::str::contains("Dog").eval(&out),
        "members should show Dog class, got:\n{out}"
    );
    assert!(
        predicate::str::contains("fetch").eval(&out),
        "members should show fetch method for Dog, got:\n{out}"
    );

    // ── 9. members on non-class should give error ───────────────────
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(test_project_root());
    cmd.arg("members").arg("create_dog").arg("--file").arg(&models_str);
    let output = cmd.output().expect("failed to run tyf");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        predicate::str::contains("not a class").eval(&stderr),
        "members should report non-class error, got stderr:\n{stderr}"
    );
}
