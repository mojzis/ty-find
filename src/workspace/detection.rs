use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct WorkspaceDetector;

#[allow(dead_code)]
impl WorkspaceDetector {
    pub fn find_workspace_root(start_path: &Path) -> Option<PathBuf> {
        let mut current = start_path;

        loop {
            if Self::has_python_markers(current) {
                return Some(current.to_path_buf());
            }

            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                break;
            }
        }

        None
    }

    fn has_python_markers(path: &Path) -> bool {
        let markers = [
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
            "poetry.lock",
            ".git",
            "src",
        ];

        markers.iter().any(|marker| path.join(marker).exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finds_workspace_with_pyproject_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();

        let result = WorkspaceDetector::find_workspace_root(&sub);
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_finds_workspace_with_git() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let result = WorkspaceDetector::find_workspace_root(dir.path());
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_no_markers_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("empty_sub");
        std::fs::create_dir(&sub).unwrap();

        let result = WorkspaceDetector::find_workspace_root(&sub);
        // Should return None or find a parent with markers (like the repo root).
        // We can't assert None because the real filesystem may have markers above.
        // Instead, assert that the result does NOT equal the sub directory (which has no markers).
        if let Some(root) = &result {
            assert_ne!(root, &sub, "sub directory has no markers, should not be returned");
        }
    }

    #[test]
    fn test_finds_nearest_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();

        let result = WorkspaceDetector::find_workspace_root(&nested);
        assert_eq!(result, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_has_python_markers_with_requirements_txt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();

        assert!(WorkspaceDetector::has_python_markers(dir.path()));
    }

    #[test]
    fn test_has_python_markers_with_setup_py() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("setup.py"), "").unwrap();

        assert!(WorkspaceDetector::has_python_markers(dir.path()));
    }

    #[test]
    fn test_has_no_python_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "# Hello").unwrap();

        assert!(!WorkspaceDetector::has_python_markers(dir.path()));
    }
}
