use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::{Child, Command};

/// Describes how to invoke `ty` — either directly or via `uvx`.
enum TyCommand {
    Direct,
    Uvx,
}

impl TyCommand {
    fn build(&self) -> Command {
        match self {
            TyCommand::Direct => Command::new("ty"),
            TyCommand::Uvx => {
                let mut cmd = Command::new("uvx");
                cmd.arg("ty");
                cmd
            }
        }
    }

    fn label(&self) -> &'static str {
        match self {
            TyCommand::Direct => "ty",
            TyCommand::Uvx => "uvx ty",
        }
    }
}

#[allow(dead_code)]
pub struct TyLspServer {
    process: Child,
    workspace_root: String,
}

#[allow(dead_code)]
impl TyLspServer {
    /// Try to find a working `ty` invocation. Checks `ty` on PATH first,
    /// then falls back to `uvx ty`.
    async fn resolve_ty_command() -> Result<TyCommand> {
        // Try direct `ty` first
        if let Ok(output) = Command::new("ty").arg("--version").output().await {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                tracing::debug!("Found ty on PATH: {}", version.trim());
                return Ok(TyCommand::Direct);
            }
        }

        tracing::debug!("ty not found on PATH, trying uvx...");

        // Fall back to `uvx ty`
        let uvx_output = Command::new("uvx")
            .arg("ty")
            .arg("--version")
            .output()
            .await
            .context(
                "Neither 'ty' nor 'uvx' found on PATH. \
                 Install ty with: uv add --dev ty",
            )?;

        if uvx_output.status.success() {
            let version = String::from_utf8_lossy(&uvx_output.stdout);
            tracing::debug!("Found ty via uvx: {}", version.trim());
            return Ok(TyCommand::Uvx);
        }

        let stderr = String::from_utf8_lossy(&uvx_output.stderr);
        anyhow::bail!(
            "ty is not available. Tried 'ty' and 'uvx ty' but neither worked.\n\
             Install it with: uv add --dev ty\n\
             uvx ty --version stderr: {}",
            stderr.trim()
        )
    }

    pub async fn start(workspace_root: &str) -> Result<Self> {
        tracing::debug!("Checking ty availability...");
        let ty_cmd = Self::resolve_ty_command().await?;

        tracing::debug!(
            "Starting ty LSP server via '{}' in workspace: {}",
            ty_cmd.label(),
            workspace_root
        );

        let process = ty_cmd
            .build()
            .arg("server")
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to spawn '{} server' in workspace '{}'",
                    ty_cmd.label(),
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
