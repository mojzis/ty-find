use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::io::Write as _;
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

#[tokio::test]
async fn test_references_by_position() {
    require_ty();

    // Find references to `hello_world` via its definition at line 1, col 5
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("references")
        .arg("-f")
        .arg(fixture_path())
        .arg("-l")
        .arg("1")
        .arg("-c")
        .arg("5");

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("test_example.py").eval(&stdout));
}

#[tokio::test]
async fn test_references_by_symbol_name() {
    require_ty();

    // Find references to `calculate_sum` by symbol name
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("references")
        .arg("calculate_sum")
        .arg("-f")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("calculate_sum").eval(&stdout));
}

#[tokio::test]
async fn test_references_multiple_symbols() {
    require_ty();

    // Find references to multiple symbols in one call (batched via daemon)
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("references")
        .arg("hello_world")
        .arg("calculate_sum")
        .arg("-f")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("hello_world").eval(&stdout));
    assert!(predicate::str::contains("calculate_sum").eval(&stdout));
}

#[tokio::test]
async fn test_references_stdin_piping() {
    require_ty();

    // Pipe symbol names via stdin using std::process::Command
    let fixture = fixture_path();
    let bin = assert_cmd::cargo::cargo_bin!("ty-find");
    let mut child = process::Command::new(bin)
        .arg("--workspace")
        .arg(workspace_root())
        .arg("references")
        .arg("--stdin")
        .arg("-f")
        .arg(&fixture)
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .expect("failed to spawn ty-find");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        writeln!(stdin, "hello_world").expect("failed to write to stdin");
        writeln!(stdin, "calculate_sum").expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("hello_world").eval(&stdout));
    assert!(predicate::str::contains("calculate_sum").eval(&stdout));
}

#[tokio::test]
async fn test_references_file_line_col_format() {
    require_ty();

    // Use file:line:col auto-detection format
    let fixture = fixture_path();
    let position = format!("{}:1:5", fixture.display());
    let mut cmd = cargo_bin_cmd!("ty-find");
    cmd.arg("--workspace").arg(workspace_root()).arg("references").arg(&position);

    let output = cmd.output().expect("failed to run ty-find");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("test_example.py").eval(&stdout));
}
