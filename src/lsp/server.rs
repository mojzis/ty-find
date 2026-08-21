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
            Self::Direct => Command::new("ty"),
            Self::Uvx => {
                let mut cmd = Command::new("uvx");
                cmd.arg("ty");
                cmd
            }
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Direct => "ty",
            Self::Uvx => "uvx ty",
        }
    }
}

/// Extract the bare version number from `ty --version` output.
///
/// `ty` prints `"ty 0.0.73\n"`; anything unexpected is passed through trimmed
/// rather than discarded, so the user still sees whatever `ty` reported.
fn parse_version(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed.strip_prefix("ty ").unwrap_or(trimmed).trim().to_string()
}

#[allow(dead_code)]
pub struct TyLspServer {
    process: Child,
    workspace_root: String,
    /// Version string reported by `ty --version` (e.g. `"0.0.73"`), captured
    /// during command resolution. Surfaced so a capability gate can name the
    /// version the user actually has.
    ty_version: String,
}

#[allow(dead_code)]
impl TyLspServer {
    /// Try to find a working `ty` invocation. Checks `ty` on PATH first,
    /// then falls back to `uvx ty`.
    async fn resolve_ty_command() -> Result<(TyCommand, String)> {
        // Try direct `ty` first
        if let Ok(output) = Command::new("ty").arg("--version").output().await {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                tracing::debug!("Found ty on PATH: {}", version.trim());
                return Ok((TyCommand::Direct, parse_version(&version)));
            }
        }

        tracing::debug!("ty not found on PATH, trying uvx...");

        // Fall back to `uvx ty`
        let uvx_output = Command::new("uvx").arg("ty").arg("--version").output().await.context(
            "Neither 'ty' nor 'uvx' found on PATH. \
                 Install ty with: uv add --dev ty",
        )?;

        if uvx_output.status.success() {
            let version = String::from_utf8_lossy(&uvx_output.stdout);
            tracing::debug!("Found ty via uvx: {}", version.trim());
            return Ok((TyCommand::Uvx, parse_version(&version)));
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
        let (ty_cmd, ty_version) = Self::resolve_ty_command().await?;

        tracing::debug!(
            "Starting ty LSP server via '{}' in workspace: {workspace_root}",
            ty_cmd.label(),
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
                    "Failed to spawn '{} server' in workspace '{workspace_root}'",
                    ty_cmd.label(),
                )
            })?;

        tracing::debug!("ty LSP server process started (pid: {:?})", process.id());

        Ok(Self { process, workspace_root: workspace_root.to_string(), ty_version })
    }

    /// The `ty` version string captured at spawn time (e.g. `"0.0.73"`).
    pub fn ty_version(&self) -> &str {
        &self.ty_version
    }

    pub fn take_stdin(&mut self) -> tokio::process::ChildStdin {
        self.process.stdin.take().expect("ty LSP server stdin not available (already taken)")
    }

    pub fn take_stdout(&mut self) -> BufReader<tokio::process::ChildStdout> {
        BufReader::new(
            self.process.stdout.take().expect("ty LSP server stdout not available (already taken)"),
        )
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

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_standard_ty_version_output() {
        assert_eq!(parse_version("ty 0.0.73\n"), "0.0.73");
    }

    #[test]
    fn parses_version_without_prefix() {
        assert_eq!(parse_version("0.0.41"), "0.0.41");
    }

    #[test]
    fn passes_through_unexpected_output_trimmed() {
        assert_eq!(parse_version("  something else  "), "something else");
    }
}
