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
async fn test_find_command() {
    require_ty();

    // Find the `add` method in test_example.py
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("find")
        .arg("add")
        .arg("--file")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("add").eval(&stdout));
}

#[tokio::test]
async fn test_json_output() {
    require_ty();

    // Find `calculate_sum` with JSON output
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("--format")
        .arg("json")
        .arg("find")
        .arg("calculate_sum")
        .arg("--file")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("uri").eval(&stdout));
    assert!(predicate::str::contains("range").eval(&stdout));
}

#[tokio::test]
async fn test_inspect_command_with_file() {
    require_ty();

    // Inspect the `hello_world` function (--file path, uses SymbolFinder)
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Definition section must show the file location
    assert!(
        predicate::str::contains("test_example.py:1:").eval(&stdout),
        "inspect should find definition, got:\n{stdout}"
    );
    // Hover/Type section must contain actual type info (not "(none)")
    assert!(
        predicate::str::contains("hello_world").eval(&stdout),
        "inspect should show type signature, got:\n{stdout}"
    );
    assert!(
        !predicate::str::contains("# Type\n(none)").eval(&stdout),
        "hover should not be empty — type info must be returned, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_command_workspace_symbols() {
    require_ty();

    // Inspect `hello_world` WITHOUT --file (uses workspace symbols + find_name_column)
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("inspect").arg("hello_world");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Definition section must show the file location
    assert!(
        predicate::str::contains("test_example.py:1:").eval(&stdout),
        "inspect should find definition, got:\n{stdout}"
    );
    // Hover/Type section must contain actual type info (not "(none)")
    assert!(
        !predicate::str::contains("# Type\n(none)").eval(&stdout),
        "hover should not be empty — type info must be returned.\n\
         If this fails, find_name_column may have returned the wrong column.\n\
         Got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_class_workspace_symbols() {
    require_ty();

    // Inspect `Calculator` class WITHOUT --file — this is the case where
    // workspace symbols return column at "class" keyword, and find_name_column
    // must correct it to the "Calculator" name position for hover to work.
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("inspect").arg("Calculator");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Should show as class kind
    assert!(
        predicate::str::contains("(class)").eval(&stdout),
        "Calculator should be identified as a class, got:\n{stdout}"
    );
    // Hover must return actual type info
    assert!(
        !predicate::str::contains("# Type\n(none)").eval(&stdout),
        "hover should not be empty for Calculator class.\n\
         This tests that find_name_column correctly shifts from 'class' keyword \
         to 'Calculator' name.\nGot:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_command_with_references() {
    require_ty();

    // Inspect `hello_world` with --references to verify all three sections
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path())
        .arg("--references");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Definition
    assert!(
        predicate::str::contains("test_example.py:1:").eval(&stdout),
        "should find hello_world definition, got:\n{stdout}"
    );
    // Hover must have actual type info
    assert!(
        !predicate::str::contains("# Type\n(none)").eval(&stdout),
        "inspect should return hover info, got:\n{stdout}"
    );
    // References section must have actual locations (not "(none)")
    assert!(
        !predicate::str::contains("# Refs\n(none)").eval(&stdout),
        "inspect --references should find usages, got:\n{stdout}"
    );
    // Should show at least 2 refs (definition + usage in main())
    assert!(
        predicate::str::contains("# Refs (").eval(&stdout),
        "references should show count, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_references_by_position() {
    require_ty();

    // Find references to `hello_world` via its definition at line 1, col 5
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("-f")
        .arg(fixture_path())
        .arg("-l")
        .arg("1")
        .arg("-c")
        .arg("5");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("test_example.py").eval(&stdout));
}

#[tokio::test]
async fn test_references_by_symbol_name() {
    require_ty();

    // Find references to `calculate_sum` by symbol name
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("calculate_sum")
        .arg("-f")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("calculate_sum").eval(&stdout));
}

#[tokio::test]
async fn test_references_multiple_symbols() {
    require_ty();

    // Find references to multiple symbols in one call (batched via daemon)
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("hello_world")
        .arg("calculate_sum")
        .arg("-f")
        .arg(fixture_path());

    let output = cmd.output().expect("failed to run tyf");
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
    let bin = assert_cmd::cargo::cargo_bin!("tyf");
    let mut child = process::Command::new(bin)
        .arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("--stdin")
        .arg("-f")
        .arg(&fixture)
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .expect("failed to spawn tyf");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        writeln!(stdin, "hello_world").expect("failed to write to stdin");
        writeln!(stdin, "calculate_sum").expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for tyf");
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
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("refs").arg(&position);

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("test_example.py").eval(&stdout));
}
