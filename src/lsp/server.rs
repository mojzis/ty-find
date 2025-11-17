use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::io::BufReader;
use anyhow::Result;

#[allow(dead_code)]
pub struct TyLspServer {
    process: Child,
    workspace_root: String,
}

#[allow(dead_code)]
impl TyLspServer {
    pub async fn start(workspace_root: &str) -> Result<Self> {
        let ty_check = Command::new("ty")
            .arg("--version")
            .output()
            .await?;

        if !ty_check.status.success() {
            anyhow::bail!("ty is not installed or not available in PATH");
        }

        let process = Command::new("ty")
            .arg("lsp")
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

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