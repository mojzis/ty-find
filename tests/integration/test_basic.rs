use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_definition_command() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    
    fs::write(&test_file, r#"
def hello_world():
    return "Hello, World!"

def main():
    result = hello_world()
    print(result)
"#).unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("definition")
        .arg(&test_file)
        .arg("--line").arg("6")
        .arg("--column").arg("15")
        .arg("--workspace").arg(temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

#[tokio::test]
async fn test_find_command() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    
    fs::write(&test_file, r#"
class Calculator:
    def add(self, a, b):
        return a + b
    
    def multiply(self, a, b):
        return a * b

calc = Calculator()
result = calc.add(1, 2)
"#).unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("find")
        .arg(&test_file)
        .arg("add")
        .arg("--workspace").arg(temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("def add"));
}

#[test]
fn test_json_output() {
    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("definition")
        .arg("nonexistent.py")
        .arg("--line").arg("1")
        .arg("--column").arg("1")
        .arg("--format").arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}