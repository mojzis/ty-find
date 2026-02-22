use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process;

/// Path to the shared test fixture at the repo root.
fn fixture_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("test_example.py")
}

/// Workspace root (repo root) used as the `--workspace` argument.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Ensure `ty` is available, either directly on PATH or via `uvx`.
/// Panics with install instructions if neither works.
fn require_ty() {
    let direct = process::Command::new("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if direct {
        return;
    }

    let via_uvx = process::Command::new("uvx")
        .arg("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    assert!(
        via_uvx,
        "ty is not installed and uvx fallback failed. Install it with: uv add --dev ty"
    );
}

#[tokio::test]
async fn test_definition_command() {
    require_ty();

    // Go to definition of `hello_world()` call on line 18
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("definition")
        .arg(fixture_path())
        .arg("--line")
        .arg("18")
        .arg("--column")
        .arg("14");

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("hello_world").eval(&stdout));
}

#[tokio::test]
async fn test_find_command() {
    require_ty();

    // Find the `add` method in test_example.py
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("find")
        .arg("add")
        .arg("--file")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("add").eval(&stdout));
}

#[tokio::test]
async fn test_json_output() {
    require_ty();

    // Go to definition of `calculate_sum()` call on line 19, with JSON output
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("--format")
        .arg("json")
        .arg("definition")
        .arg(fixture_path())
        .arg("--line")
        .arg("19")
        .arg("--column")
        .arg("13");

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("uri").eval(&stdout));
    assert!(predicate::str::contains("range").eval(&stdout));
}

#[tokio::test]
async fn test_inspect_command() {
    require_ty();

    // Inspect the `hello_world` function
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("hello_world").eval(&stdout));
}
