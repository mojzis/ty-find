//! Ripgrep-based symbol existence check for early termination.
//!
//! When the LSP returns empty/null results for a symbol, we use `rg` as a fast
//! negative filter: if the symbol text doesn't appear in any `.py` file in the
//! workspace, there's no point retrying — the symbol provably doesn't exist.
//!
//! This is a **one-directional optimization**: `rg` returning zero matches
//! guarantees non-existence. `rg` returning matches does NOT guarantee the
//! symbol exists (it could be in a comment or string), so we continue retries
//! in that case.

use std::path::Path;
use std::process::Command;

/// Check whether a symbol name appears in any Python file under `workspace_root`.
///
/// Returns `false` only when `rg` confirms the symbol does not exist (exit code 1).
/// Returns `true` (conservative / "might exist") when:
/// - `rg` finds matches (exit code 0)
/// - `rg` is not found on PATH
/// - `rg` returns any error
/// - The symbol name is empty
pub fn symbol_might_exist_in_workspace(symbol: &str, workspace_root: &Path) -> bool {
    if symbol.is_empty() {
        tracing::debug!("rg: empty symbol name, skipping existence check");
        return true;
    }

    let result = Command::new("rg")
        .arg("--count")
        .arg("--word-regexp")
        .arg("--fixed-strings")
        .arg("--type")
        .arg("py")
        .arg(symbol)
        .arg(workspace_root)
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                // Exit code 0: matches found — symbol might exist
                tracing::debug!("rg: symbol '{symbol}' found in .py files, continuing retries");
                true
            } else if output.status.code() == Some(1) {
                // Exit code 1: no matches — symbol definitely does not exist
                tracing::debug!(
                    "rg: symbol '{symbol}' not found in any .py file, skipping retries"
                );
                false
            } else {
                // Other exit code: rg error — fall back to retries
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::debug!(
                    "rg: unexpected exit code {:?} for symbol '{symbol}': {stderr}",
                    output.status.code()
                );
                true
            }
        }
        Err(e) => {
            // rg not found on PATH or failed to execute
            tracing::debug!("rg not found on PATH, skipping existence check: {e}");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workspace(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("Failed to create dir");
            }
            fs::write(&path, content).expect("Failed to write file");
        }
        dir
    }

    #[test]
    fn test_symbol_found_in_workspace() {
        let ws = create_test_workspace(&[("example.py", "def greet():\n    pass\n")]);
        assert!(symbol_might_exist_in_workspace("greet", ws.path()));
    }

    #[test]
    fn test_symbol_not_found_in_workspace() {
        let ws = create_test_workspace(&[("example.py", "def greet():\n    pass\n")]);
        assert!(!symbol_might_exist_in_workspace("nonexistent_symbol_xyz", ws.path()));
    }

    #[test]
    fn test_word_boundary_prevents_partial_match() {
        let ws = create_test_workspace(&[(
            "example.py",
            "def calculate_sum(a, b):\n    return a + b\n",
        )]);
        // "sum" should NOT match "calculate_sum" with --word-regexp
        assert!(!symbol_might_exist_in_workspace("sum", ws.path()));
    }

    #[test]
    fn test_dunder_symbol_matches() {
        let ws = create_test_workspace(&[(
            "example.py",
            "class Foo:\n    def __init__(self):\n        pass\n",
        )]);
        // __init__ should match with --word-regexp --fixed-strings
        // because _ is a word character, so \b__init__\b works
        assert!(symbol_might_exist_in_workspace("__init__", ws.path()));
    }

    #[test]
    fn test_dunder_symbol_not_present() {
        let ws = create_test_workspace(&[("example.py", "x = 1\n")]);
        assert!(!symbol_might_exist_in_workspace("__init__", ws.path()));
    }

    #[test]
    fn test_empty_symbol_returns_true() {
        let ws = create_test_workspace(&[("example.py", "x = 1\n")]);
        // Empty symbol should conservatively return true (skip the check)
        assert!(symbol_might_exist_in_workspace("", ws.path()));
    }

    #[test]
    fn test_only_searches_python_files() {
        let ws = create_test_workspace(&[
            ("readme.txt", "greet is mentioned here\n"),
            ("config.json", "{\"greet\": true}\n"),
        ]);
        // Symbol only in non-Python files should not be found
        assert!(!symbol_might_exist_in_workspace("greet", ws.path()));
    }

    #[test]
    fn test_symbol_with_regex_metacharacters() {
        let ws =
            create_test_workspace(&[("example.py", "# pattern: foo.*bar\ndef normal(): pass\n")]);
        // --fixed-strings prevents regex interpretation
        // "foo.*bar" as a literal should be found in the comment
        assert!(symbol_might_exist_in_workspace("foo.*bar", ws.path()));
        // But a symbol that doesn't exist should still return false
        assert!(!symbol_might_exist_in_workspace("baz.*qux", ws.path()));
    }

    #[test]
    fn test_workspace_with_spaces_in_path() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let spaced_dir = dir.path().join("my project");
        fs::create_dir_all(&spaced_dir).expect("Failed to create dir");
        fs::write(spaced_dir.join("example.py"), "def hello(): pass\n")
            .expect("Failed to write file");

        assert!(symbol_might_exist_in_workspace("hello", &spaced_dir));
        assert!(!symbol_might_exist_in_workspace("nonexistent", &spaced_dir));
    }
}
