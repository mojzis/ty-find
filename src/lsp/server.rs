use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::{Child, Command};

#[allow(dead_code)]
pub struct TyLspServer {
    process: Child,
    workspace_root: String,
}

#[allow(dead_code)]
impl TyLspServer {
    pub async fn start(workspace_root: &str) -> Result<Self> {
        tracing::debug!("Checking ty availability...");
        let ty_check = Command::new("ty").arg("--version").output().await.context(
            "Failed to run 'ty --version'. Is ty installed? Install it with: uv add --dev ty",
        )?;

        if !ty_check.status.success() {
            let stderr = String::from_utf8_lossy(&ty_check.stderr);
            anyhow::bail!(
                "ty is not installed or not available in PATH. \
                 Install it with: uv add --dev ty\n\
                 ty --version stderr: {}",
                stderr.trim()
            );
        }

        let ty_version = String::from_utf8_lossy(&ty_check.stdout);
        tracing::debug!("Found ty: {}", ty_version.trim());
        tracing::debug!("Starting ty LSP server in workspace: {}", workspace_root);

        let process = Command::new("ty")
            .arg("server")
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to spawn 'ty server' in workspace '{}'",
                    workspace_root
                )
            })?;

        tracing::debug!("ty LSP server process started (pid: {:?})", process.id());

        Ok(Self {
            process,
            workspace_root: workspace_root.to_string(),
        })
    }

    pub fn stdin(&mut self) -> &mut tokio::process::ChildStdin {
        self.process.stdin.as_mut().unwrap()
    }

    pub fn stdout(&mut self) -> BufReader<tokio::process::ChildStdout> {
        BufReader::new(self.process.stdout.take().unwrap())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}

impl Drop for TyLspServer {
    fn drop(&mut self) {
        let _ = self.process.start_kill();
    }
}
