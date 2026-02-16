use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process;
use tempfile::TempDir;

/// Ensure `ty` is available on PATH. Panics with install instructions if missing.
fn require_ty() {
    let available = process::Command::new("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    assert!(
        available,
        "ty is not installed. Install it with: pip install ty"
    );
}

#[tokio::test]
async fn test_definition_command() {
    require_ty();

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");

    fs::write(
        &test_file,
        r#"
def hello_world():
    return "Hello, World!"

def main():
    result = hello_world()
    print(result)
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("--workspace")
        .arg(temp_dir.path())
        .arg("definition")
        .arg(&test_file)
        .arg("--line")
        .arg("6")
        .arg("--column")
        .arg("15");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

#[tokio::test]
async fn test_find_command() {
    require_ty();

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");

    fs::write(
        &test_file,
        r#"
class Calculator:
    def add(self, a, b):
        return a + b

    def multiply(self, a, b):
        return a * b

calc = Calculator()
result = calc.add(1, 2)
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("--workspace")
        .arg(temp_dir.path())
        .arg("find")
        .arg(&test_file)
        .arg("add");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("add"));
}

#[tokio::test]
async fn test_json_output() {
    require_ty();

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");

    fs::write(
        &test_file,
        r#"
def greet():
    return "hi"

greet()
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("--workspace")
        .arg(temp_dir.path())
        .arg("--format")
        .arg("json")
        .arg("definition")
        .arg(&test_file)
        .arg("--line")
        .arg("5")
        .arg("--column")
        .arg("1");

    // JSON output should contain a valid location with uri and range fields
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("uri"))
        .stdout(predicate::str::contains("range"));
}
