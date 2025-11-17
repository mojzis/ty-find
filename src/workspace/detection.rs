use std::path::{Path, PathBuf};
use anyhow::Result;

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

    pub async fn check_ty_availability() -> Result<String> {
        let output = tokio::process::Command::new("ty")
            .arg("--version")
            .output()
            .await?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            Ok(version.trim().to_string())
        } else {
            anyhow::bail!("ty is not available or failed to run")
        }
    }
}