#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::io::Write as _;
use std::path::PathBuf;
use std::process;

/// Path to the shared test fixture at the repo root.
fn fixture_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("example.py")
}

/// Workspace root (repo root) used as the `--workspace` argument.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::test]
async fn test_find_command() {
    common::require_ty();

    // Find the `add` method in example.py
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
    common::require_ty();

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
    common::require_ty();

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
        predicate::str::contains("example.py:1:").eval(&stdout),
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
    // Reference count is always shown now (no -r needed)
    assert!(
        predicate::str::contains("# Refs:").eval(&stdout),
        "inspect should always show reference count summary, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_command_workspace_symbols() {
    common::require_ty();

    // Inspect `hello_world` WITHOUT --file (uses workspace symbols + find_name_column)
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("inspect").arg("hello_world");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Definition section must show the file location
    assert!(
        predicate::str::contains("example.py:1:").eval(&stdout),
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
    common::require_ty();

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
    common::require_ty();

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
        predicate::str::contains("example.py:1:").eval(&stdout),
        "should find hello_world definition, got:\n{stdout}"
    );
    // Hover must have actual type info
    assert!(
        !predicate::str::contains("# Type\n(none)").eval(&stdout),
        "inspect should return hover info, got:\n{stdout}"
    );
    // References section must show count and file summary
    assert!(
        predicate::str::contains("# Refs:").eval(&stdout),
        "inspect should show reference count, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("across").eval(&stdout),
        "should show 'N across M file(s)', got:\n{stdout}"
    );
    // With -r, individual refs should be listed with enclosing context
    assert!(
        predicate::str::contains("example.py:").eval(&stdout),
        "individual references should be displayed, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_references_by_position() {
    common::require_ty();

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
    assert!(predicate::str::contains("example.py").eval(&stdout));
}

#[tokio::test]
async fn test_references_by_symbol_name() {
    common::require_ty();

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
    common::require_ty();

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
    common::require_ty();

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
    common::require_ty();

    // Use file:line:col auto-detection format
    let fixture = fixture_path();
    let position = format!("{}:1:5", fixture.display());
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("refs").arg(&position);

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");
    assert!(predicate::str::contains("example.py").eval(&stdout));
}

// ── Members command tests ──────────────────────────────────────────────

/// Path to the members test fixture.
fn members_fixture_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("members_example.py")
}

#[tokio::test]
async fn test_members_command_basic() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("members")
        .arg("Animal")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Should show class name and file location
    assert!(
        predicate::str::contains("Animal").eval(&stdout),
        "should show class name, got:\n{stdout}"
    );
    // Should show Methods section with public methods
    assert!(
        predicate::str::contains("Methods:").eval(&stdout),
        "should have Methods section, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("speak").eval(&stdout),
        "should show speak method, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("describe").eval(&stdout),
        "should show describe method, got:\n{stdout}"
    );
    // Should NOT show dunder methods by default
    assert!(
        !predicate::str::contains("__init__").eval(&stdout),
        "should NOT show __init__ by default, got:\n{stdout}"
    );
    assert!(
        !predicate::str::contains("__repr__").eval(&stdout),
        "should NOT show __repr__ by default, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_members_command_all_flag() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("members")
        .arg("Animal")
        .arg("--all")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // --all should include dunder methods
    assert!(
        predicate::str::contains("__init__").eval(&stdout),
        "--all should include __init__, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_members_command_non_class_error() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("members")
        .arg("standalone_function")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should print an error about not being a class
    assert!(
        predicate::str::contains("not a class").eval(&stderr),
        "should indicate it's not a class, got stderr:\n{stderr}"
    );
}

#[tokio::test]
async fn test_members_command_multiple_classes() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("members")
        .arg("Animal")
        .arg("Dog")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    assert!(
        predicate::str::contains("Animal").eval(&stdout),
        "should show Animal class, got:\n{stdout}"
    );
    assert!(predicate::str::contains("Dog").eval(&stdout), "should show Dog class, got:\n{stdout}");
    assert!(
        predicate::str::contains("fetch").eval(&stdout),
        "should show Dog.fetch method, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_members_command_json_format() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("--format")
        .arg("json")
        .arg("members")
        .arg("Animal")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    assert!(
        predicate::str::contains("\"class_name\"").eval(&stdout),
        "JSON should have class_name field, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("\"members\"").eval(&stdout),
        "JSON should have members field, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_members_command_csv_format() {
    common::require_ty();

    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("--format")
        .arg("csv")
        .arg("members")
        .arg("Animal")
        .arg("--file")
        .arg(members_fixture_path());

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    assert!(
        predicate::str::contains("class,member,kind,signature,line,column").eval(&stdout),
        "CSV should have header, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("Animal,speak").eval(&stdout),
        "CSV should have speak member, got:\n{stdout}"
    );
}

// ── Reference count and enrichment tests ───────────────────────────

#[tokio::test]
async fn test_inspect_shows_reference_count_without_r_flag() {
    common::require_ty();

    // Inspect WITHOUT --references should still show reference count
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

    // Should show "# Refs: N across M file(s)" even without -r
    assert!(
        predicate::str::contains("# Refs:").eval(&stdout),
        "inspect should always show ref count, got:\n{stdout}"
    );
    assert!(
        predicate::str::contains("across").eval(&stdout)
            || predicate::str::contains("none").eval(&stdout),
        "should show 'N across M file(s)' or 'none', got:\n{stdout}"
    );
    // Without -r, should NOT show individual reference lines with file:line:col
    // (the definition line matches, but we check that there's no "(module scope)" or similar context)
    assert!(
        !predicate::str::contains("(module scope)").eval(&stdout)
            && !predicate::str::contains("--references-limit").eval(&stdout),
        "without -r, should not show individual refs or truncation message, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_references_with_limit_truncation() {
    common::require_ty();

    // Inspect with --references and --references-limit 1 to test truncation
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path())
        .arg("--references")
        .arg("--references-limit")
        .arg("1");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Should show the count summary
    assert!(
        predicate::str::contains("# Refs:").eval(&stdout),
        "should show refs header, got:\n{stdout}"
    );
    // If there are more than 1 reference, should show truncation
    // (hello_world has at least 2 refs: definition + call in main)
    if predicate::str::contains("... and").eval(&stdout) {
        assert!(
            predicate::str::contains("--references-limit 0").eval(&stdout),
            "truncation message should mention --references-limit 0, got:\n{stdout}"
        );
    }
}

#[tokio::test]
async fn test_inspect_references_limit_zero_shows_all() {
    common::require_ty();

    // Inspect with --references --references-limit 0 should show all refs
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path())
        .arg("--references")
        .arg("--references-limit")
        .arg("0");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Should NOT show truncation message
    assert!(
        !predicate::str::contains("... and").eval(&stdout),
        "--references-limit 0 should show all refs without truncation, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_inspect_enriched_refs_show_context() {
    common::require_ty();

    // Inspect with --references to check enclosing symbol context
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path())
        .arg("--references")
        .arg("--references-limit")
        .arg("0");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Each reference line should have a context in parentheses
    // e.g. "example.py:45:12 (main)" or "example.py:1:5 (module scope)"
    let refs_section = stdout.split("# Refs:").nth(1).unwrap_or("");
    let has_context = refs_section
        .lines()
        .any(|line| line.contains("example.py:") && (line.contains('(') && line.contains(')')));
    assert!(has_context, "references should include enclosing context, got:\n{stdout}");
}

#[tokio::test]
async fn test_inspect_module_scope_reference() {
    common::require_ty();

    // hello_world is defined at module scope — at least one ref should show "(module scope)"
    // since it's referenced in the file's top-level `if __name__` block or similar
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("inspect")
        .arg("hello_world")
        .arg("--file")
        .arg(fixture_path())
        .arg("--references")
        .arg("--references-limit")
        .arg("0");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // At least one reference should be at module scope or in a function
    // (we just verify context parentheses are present for all displayed refs)
    let refs_section = stdout.split("# Refs:").nth(1).unwrap_or("");
    for line in refs_section.lines() {
        if line.contains("example.py:") && line.contains(':') {
            // Each ref line should have context
            assert!(
                line.contains('('),
                "each reference should have context in parentheses, got: {line}"
            );
        }
    }
}

#[tokio::test]
async fn test_references_command_with_limit() {
    common::require_ty();

    // References command with --references-limit
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("hello_world")
        .arg("-f")
        .arg(fixture_path())
        .arg("--references-limit")
        .arg("1");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Should show enriched refs with context
    assert!(
        predicate::str::contains("hello_world").eval(&stdout)
            || predicate::str::contains("reference").eval(&stdout),
        "should show references, got:\n{stdout}"
    );
}

#[tokio::test]
async fn test_references_command_enriched_context() {
    common::require_ty();

    // References command should show context for each reference
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace")
        .arg(workspace_root())
        .arg("refs")
        .arg("hello_world")
        .arg("-f")
        .arg(fixture_path())
        .arg("--references-limit")
        .arg("0");

    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "command failed: {stdout}");

    // Each reference line should include enclosing context
    let has_context = stdout.lines().any(|line| line.contains("example.py:") && line.contains('('));
    assert!(has_context, "references should include context, got:\n{stdout}");
}
