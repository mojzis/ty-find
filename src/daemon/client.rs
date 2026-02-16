//! Daemon client for communicating with the persistent ty-find daemon.
//!
//! This module provides a client that connects to the daemon via Unix domain
//! sockets and sends JSON-RPC 2.0 requests. The client handles auto-starting
//! the daemon if it's not already running.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use super::protocol::*;

/// Default timeout for daemon operations (5 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for daemon startup (2 seconds).
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum number of startup retry attempts.
const MAX_STARTUP_RETRIES: usize = 20;

/// Delay between startup retry attempts (100ms).
const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Client for communicating with the ty-find daemon.
///
/// The client connects to the daemon via a Unix domain socket and sends
/// JSON-RPC 2.0 requests. Messages are framed using Content-Length headers
/// similar to the LSP protocol.
///
/// # Example
/// ```no_run
/// use ty_find::daemon::client::DaemonClient;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut client = DaemonClient::connect().await?;
///
/// let result = client.execute_hover(
///     PathBuf::from("/workspace"),
///     "file.py".to_string(),
///     10,
///     5,
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct DaemonClient {
    /// Path to the Unix domain socket.
    socket_path: PathBuf,

    /// Connection to the daemon.
    stream: UnixStream,
}

impl DaemonClient {
    /// Connect to an existing daemon.
    ///
    /// Returns an error if the daemon is not running or the socket doesn't exist.
    ///
    /// # Errors
    /// - Socket file doesn't exist
    /// - Connection to socket failed
    /// - Daemon is not responsive
    pub async fn connect() -> Result<Self> {
        let socket_path = get_socket_path()?;

        let stream = UnixStream::connect(&socket_path)
            .await
            .context("Failed to connect to daemon socket")?;

        tracing::debug!("Connected to daemon at {}", socket_path.display());

        Ok(Self {
            socket_path,
            stream,
        })
    }

    /// Send a JSON-RPC request to the daemon and wait for response.
    ///
    /// This method handles the low-level protocol details including:
    /// - Creating the JSON-RPC request
    /// - Framing with Content-Length header
    /// - Sending over the Unix socket
    /// - Reading the framed response
    /// - Parsing the JSON-RPC response
    ///
    /// # Arguments
    /// - `method`: The daemon method to invoke
    /// - `params`: Method-specific parameters as JSON value
    ///
    /// # Returns
    /// The JSON-RPC response from the daemon
    ///
    /// # Errors
    /// - Timeout waiting for response
    /// - IO error communicating with daemon
    /// - JSON parsing error
    /// - Daemon returned an error response
    pub async fn send_request(&mut self, method: Method, params: Value) -> Result<DaemonResponse> {
        let request = DaemonRequest::new(method, params);

        // Serialize request to JSON
        let request_json =
            serde_json::to_string(&request).context("Failed to serialize request")?;

        // Frame with Content-Length header
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            request_json.len(),
            request_json
        );

        // Send request with timeout
        timeout(DEFAULT_TIMEOUT, async {
            self.stream
                .write_all(message.as_bytes())
                .await
                .context("Failed to write request to daemon")?;

            tracing::debug!("Sent request: method={}", method.as_str());

            // Read response
            self.read_response().await
        })
        .await
        .context("Request timed out")?
    }

    /// Read a framed JSON-RPC response from the daemon.
    ///
    /// Expects the response to be framed with a Content-Length header:
    /// ```text
    /// Content-Length: 123\r\n
    /// \r\n
    /// {"jsonrpc":"2.0",...}
    /// ```
    async fn read_response(&mut self) -> Result<DaemonResponse> {
        let mut reader = BufReader::new(&mut self.stream);

        // Read Content-Length header
        let mut header_line = String::new();
        reader
            .read_line(&mut header_line)
            .await
            .context("Failed to read Content-Length header")?;

        // Parse content length
        let content_length = header_line
            .trim()
            .strip_prefix("Content-Length: ")
            .context("Invalid header: missing Content-Length")?
            .parse::<usize>()
            .context("Invalid Content-Length value")?;

        // Read empty line
        let mut empty_line = String::new();
        reader
            .read_line(&mut empty_line)
            .await
            .context("Failed to read header separator")?;

        if empty_line.trim() != "" {
            anyhow::bail!("Expected empty line after Content-Length header");
        }

        // Read response body
        let mut body = vec![0u8; content_length];
        reader
            .read_exact(&mut body)
            .await
            .context("Failed to read response body")?;

        // Parse JSON response
        let response: DaemonResponse =
            serde_json::from_slice(&body).context("Failed to parse JSON response")?;

        tracing::debug!("Received response: id={}", response.id);

        Ok(response)
    }

    /// Execute a hover request.
    ///
    /// Returns type information and documentation at a specific position in a file.
    ///
    /// # Arguments
    /// - `workspace`: Workspace root directory
    /// - `file`: File path (absolute or relative to workspace)
    /// - `line`: Line number (0-based)
    /// - `column`: Column number (0-based)
    pub async fn execute_hover(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
    ) -> Result<HoverResult> {
        let params = HoverParams {
            workspace,
            file: PathBuf::from(file),
            line,
            column,
        };

        let params_value =
            serde_json::to_value(params).context("Failed to serialize hover params")?;

        let response = self.send_request(Method::Hover, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize hover result")
    }

    /// Execute a definition request.
    ///
    /// Returns the location where a symbol is defined.
    ///
    /// # Arguments
    /// - `workspace`: Workspace root directory
    /// - `file`: File path (absolute or relative to workspace)
    /// - `line`: Line number (0-based)
    /// - `column`: Column number (0-based)
    pub async fn execute_definition(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
    ) -> Result<DefinitionResult> {
        let params = DefinitionParams {
            workspace,
            file: PathBuf::from(file),
            line,
            column,
        };

        let params_value =
            serde_json::to_value(params).context("Failed to serialize definition params")?;

        let response = self.send_request(Method::Definition, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize definition result")
    }

    /// Execute a workspace symbols request.
    ///
    /// Searches for symbols matching a query across the entire workspace.
    ///
    /// # Arguments
    /// - `workspace`: Workspace root directory
    /// - `query`: Search query (can be fuzzy)
    pub async fn execute_workspace_symbols(
        &mut self,
        workspace: PathBuf,
        query: String,
    ) -> Result<WorkspaceSymbolsResult> {
        let params = WorkspaceSymbolsParams {
            workspace,
            query,
            limit: None,
        };

        let params_value =
            serde_json::to_value(params).context("Failed to serialize workspace symbols params")?;

        let response = self
            .send_request(Method::WorkspaceSymbols, params_value)
            .await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize workspace symbols result")
    }

    /// Execute a document symbols request.
    ///
    /// Returns an outline of all symbols in a file.
    ///
    /// # Arguments
    /// - `workspace`: Workspace root directory
    /// - `file`: File path (absolute or relative to workspace)
    pub async fn execute_document_symbols(
        &mut self,
        workspace: PathBuf,
        file: String,
    ) -> Result<DocumentSymbolsResult> {
        let params = DocumentSymbolsParams {
            workspace,
            file: PathBuf::from(file),
        };

        let params_value =
            serde_json::to_value(params).context("Failed to serialize document symbols params")?;

        let response = self
            .send_request(Method::DocumentSymbols, params_value)
            .await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize document symbols result")
    }

    /// Execute a references request.
    ///
    /// Returns all locations where a symbol at the given position is referenced.
    ///
    /// # Arguments
    /// - `workspace`: Workspace root directory
    /// - `file`: File path (absolute or relative to workspace)
    /// - `line`: Line number (0-based)
    /// - `column`: Column number (0-based)
    /// - `include_declaration`: Whether to include the declaration in results
    pub async fn execute_references(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
        include_declaration: bool,
    ) -> Result<ReferencesResult> {
        let params = ReferencesParams {
            workspace,
            file: PathBuf::from(file),
            line,
            column,
            include_declaration,
        };

        let params_value =
            serde_json::to_value(params).context("Failed to serialize references params")?;

        let response = self.send_request(Method::References, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize references result")
    }

    /// Send a ping request to check daemon health.
    ///
    /// Returns daemon status information including uptime and cache size.
    pub async fn ping(&mut self) -> Result<PingResult> {
        let params = PingParams {};

        let params_value =
            serde_json::to_value(params).context("Failed to serialize ping params")?;

        let response = self.send_request(Method::Ping, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result).context("Failed to deserialize ping result")
    }

    /// Send a shutdown request to gracefully stop the daemon.
    pub async fn shutdown(&mut self) -> Result<()> {
        let params = ShutdownParams {};

        let params_value =
            serde_json::to_value(params).context("Failed to serialize shutdown params")?;

        let response = self.send_request(Method::Shutdown, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        tracing::info!("Daemon shutdown requested");
        Ok(())
    }
}

/// Ensure the daemon is running, starting it if necessary.
///
/// This function:
/// 1. Checks if the daemon socket exists
/// 2. If not, spawns the daemon in the background
/// 3. Waits for the daemon to start (up to DAEMON_STARTUP_TIMEOUT)
/// 4. Returns once the daemon is responsive
///
/// # Errors
/// - Failed to spawn daemon process
/// - Daemon failed to start within timeout
pub async fn ensure_daemon_running() -> Result<()> {
    let socket_path = get_socket_path()?;

    // Check if socket already exists and is connectable
    if socket_path.exists() {
        match DaemonClient::connect().await {
            Ok(_) => {
                tracing::debug!("Daemon already running");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("Socket exists but connection failed: {}", e);
                // Try to clean up stale socket
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    // Spawn daemon in background
    tracing::info!("Starting daemon...");
    spawn_daemon(&socket_path)?;

    // Wait for daemon to start
    for i in 0..MAX_STARTUP_RETRIES {
        tokio::time::sleep(STARTUP_RETRY_DELAY).await;

        if socket_path.exists() {
            match timeout(Duration::from_millis(500), DaemonClient::connect()).await {
                Ok(Ok(_)) => {
                    tracing::info!("Daemon started successfully");
                    return Ok(());
                }
                Ok(Err(e)) => {
                    tracing::debug!("Connection attempt {} failed: {}", i + 1, e);
                }
                Err(_) => {
                    tracing::debug!("Connection attempt {} timed out", i + 1);
                }
            }
        }
    }

    anyhow::bail!("Daemon failed to start within {:?}", DAEMON_STARTUP_TIMEOUT)
}

/// Spawn the daemon process in the background.
///
/// The daemon is started as a detached background process that will
/// continue running after the CLI process exits.
fn spawn_daemon(socket_path: &Path) -> Result<()> {
    use std::process::{Command, Stdio};

    // Get the current executable path
    let exe = std::env::current_exe().context("Failed to get current executable path")?;

    // Spawn daemon process
    // TODO: This will need to be updated once we implement the actual daemon server
    // For now, this is a placeholder that shows the intended behavior
    let child = Command::new(exe)
        .arg("daemon")
        .arg("start")
        .arg("--socket")
        .arg(socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn daemon process")?;

    // Detach the process (it will continue running after parent exits)
    // Note: The process is already detached via spawn. For true daemonization
    // on Unix, we could use std::os::unix::process::CommandExt::pre_exec
    // to call setsid() if needed in the future.

    tracing::debug!("Spawned daemon process with PID {}", child.id());

    Ok(())
}

/// Get the path to the daemon socket.
///
/// Returns `/tmp/ty-find-{uid}.sock` on Unix systems where {uid} is the
/// current user ID. This ensures each user has their own daemon instance.
///
/// # Errors
/// - Failed to get current user ID
pub fn get_socket_path() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        // Get current user ID for socket isolation
        let uid = unsafe { libc::getuid() };
        let socket_name = format!("ty-find-{}.sock", uid);
        let socket_path = PathBuf::from("/tmp").join(socket_name);
        Ok(socket_path)
    }

    #[cfg(not(unix))]
    {
        // Windows named pipe support would go here
        anyhow::bail!("Daemon mode is only supported on Unix systems")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_socket_path() {
        let socket_path = get_socket_path().unwrap();
        assert!(socket_path.to_string_lossy().contains("ty-find"));
        assert!(socket_path.to_string_lossy().ends_with(".sock"));
    }

    #[test]
    fn test_socket_path_contains_uid() {
        #[cfg(unix)]
        {
            let uid = unsafe { libc::getuid() };
            let socket_path = get_socket_path().unwrap();
            let path_str = socket_path.to_string_lossy();
            assert!(path_str.contains(&uid.to_string()));
        }
    }

    #[tokio::test]
    async fn test_daemon_request_creation() {
        let params = HoverParams {
            workspace: PathBuf::from("/workspace"),
            file: PathBuf::from("file.py"),
            line: 10,
            column: 5,
        };

        let params_value = serde_json::to_value(params).unwrap();
        let request = DaemonRequest::new(Method::Hover, params_value);

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, Method::Hover);
        assert!(request.id > 0);
    }

    #[tokio::test]
    async fn test_response_parsing() {
        let response_json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hover": null
            }
        }"#;

        let response: DaemonResponse = serde_json::from_str(response_json).unwrap();
        assert!(response.is_success());
        assert!(!response.is_error());
        assert_eq!(response.id, 1);
    }

    #[tokio::test]
    async fn test_error_response_parsing() {
        let response_json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32000,
                "message": "File not found",
                "data": {"file": "/path/to/file.py"}
            }
        }"#;

        let response: DaemonResponse = serde_json::from_str(response_json).unwrap();
        assert!(response.is_error());
        assert!(!response.is_success());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32000);
        assert_eq!(error.message, "File not found");
    }
}
