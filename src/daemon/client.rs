//! Daemon client for communicating with the persistent ty-find daemon.
//!
//! This module provides a client that connects to the daemon via Unix domain
//! sockets and sends JSON-RPC 2.0 requests. The client handles auto-starting
//! the daemon if it's not already running.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use super::protocol::{
    DaemonRequest, DaemonResponse, DefinitionParams, DefinitionResult, DocumentSymbolsParams,
    DocumentSymbolsResult, HoverParams, HoverResult, Method, PingParams, PingResult,
    ReferencesParams, ReferencesResult, ShutdownParams, ShutdownResult, WorkspaceSymbolsParams,
    WorkspaceSymbolsResult,
};

/// Default timeout for daemon operations (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

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
    #[allow(dead_code)]
    socket_path: PathBuf,

    /// Connection to the daemon.
    stream: UnixStream,

    /// Timeout for daemon operations.
    timeout: Duration,
}

impl DaemonClient {
    /// Connect to an existing daemon with the default timeout.
    ///
    /// Returns an error if the daemon is not running or the socket doesn't exist.
    pub async fn connect() -> Result<Self> {
        Self::connect_with_timeout(DEFAULT_TIMEOUT).await
    }

    /// Connect to an existing daemon with a custom timeout.
    pub async fn connect_with_timeout(timeout: Duration) -> Result<Self> {
        let socket_path = get_socket_path()?;

        let stream = UnixStream::connect(&socket_path)
            .await
            .context("Failed to connect to daemon socket")?;

        tracing::debug!("Connected to daemon at {}", socket_path.display());

        Ok(Self { socket_path, stream, timeout })
    }

    /// Send a JSON-RPC request to the daemon and wait for response.
    pub async fn send_request(&mut self, method: Method, params: Value) -> Result<DaemonResponse> {
        let request = DaemonRequest::new(method, params);

        // Serialize request to JSON
        let request_json =
            serde_json::to_string(&request).context("Failed to serialize request")?;

        // Frame with Content-Length header
        let message = format!("Content-Length: {}\r\n\r\n{request_json}", request_json.len());

        // Send request with timeout
        timeout(self.timeout, async {
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
        reader.read_line(&mut header_line).await.context("Failed to read Content-Length header")?;

        // Parse content length
        let content_length = header_line
            .trim()
            .strip_prefix("Content-Length: ")
            .context("Invalid header: missing Content-Length")?
            .parse::<usize>()
            .context("Invalid Content-Length value")?;

        // Read empty line
        let mut empty_line = String::new();
        reader.read_line(&mut empty_line).await.context("Failed to read header separator")?;

        if !empty_line.trim().is_empty() {
            anyhow::bail!("Expected empty line after Content-Length header");
        }

        // Read response body
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await.context("Failed to read response body")?;

        // Parse JSON response
        let response: DaemonResponse =
            serde_json::from_slice(&body).context("Failed to parse JSON response")?;

        tracing::debug!("Received response: id={}", response.id);

        Ok(response)
    }

    /// Send a typed request and deserialize the response.
    ///
    /// Handles the common pattern: serialize params → send → check error → deserialize result.
    async fn execute<P: serde::Serialize, R: DeserializeOwned>(
        &mut self,
        method: Method,
        params: P,
    ) -> Result<R> {
        let params_value = serde_json::to_value(params)
            .with_context(|| format!("Failed to serialize {} params", method.as_str()))?;

        let response = self.send_request(method, params_value).await?;

        if let Some(error) = response.error {
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;

        serde_json::from_value(result)
            .with_context(|| format!("Failed to deserialize {} result", method.as_str()))
    }

    /// Execute a hover request.
    pub async fn execute_hover(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
    ) -> Result<HoverResult> {
        let params = HoverParams { workspace, file: PathBuf::from(file), line, column };
        self.execute(Method::Hover, params).await
    }

    /// Execute a definition request.
    pub async fn execute_definition(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
    ) -> Result<DefinitionResult> {
        let params = DefinitionParams { workspace, file: PathBuf::from(file), line, column };
        self.execute(Method::Definition, params).await
    }

    /// Execute a workspace symbols request.
    pub async fn execute_workspace_symbols(
        &mut self,
        workspace: PathBuf,
        query: String,
    ) -> Result<WorkspaceSymbolsResult> {
        let params = WorkspaceSymbolsParams { workspace, query, limit: None };
        self.execute(Method::WorkspaceSymbols, params).await
    }

    /// Execute a document symbols request.
    pub async fn execute_document_symbols(
        &mut self,
        workspace: PathBuf,
        file: String,
    ) -> Result<DocumentSymbolsResult> {
        let params = DocumentSymbolsParams { workspace, file: PathBuf::from(file) };
        self.execute(Method::DocumentSymbols, params).await
    }

    /// Execute a references request.
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
        self.execute(Method::References, params).await
    }

    /// Send a ping request to check daemon health.
    pub async fn ping(&mut self) -> Result<PingResult> {
        self.execute(Method::Ping, PingParams {}).await
    }

    /// Send a shutdown request to gracefully stop the daemon.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _: ShutdownResult = self.execute(Method::Shutdown, ShutdownParams {}).await?;
        tracing::info!("Daemon shutdown requested");
        Ok(())
    }
}

/// Ensure the daemon is running, starting it if necessary.
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
                tracing::warn!("Socket exists but connection failed: {e}");
                // Try to clean up stale socket
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    // Spawn daemon in background
    tracing::info!("Starting daemon...");
    spawn_daemon()?;

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
                    tracing::debug!("Connection attempt {} failed: {e}", i + 1);
                }
                Err(_) => {
                    tracing::debug!("Connection attempt {} timed out", i + 1);
                }
            }
        }
    }

    anyhow::bail!("Daemon failed to start within {DAEMON_STARTUP_TIMEOUT:?}")
}

/// Spawn the daemon process in the background.
pub fn spawn_daemon() -> Result<()> {
    use std::process::{Command, Stdio};

    // Get the current executable path
    let exe = std::env::current_exe().context("Failed to get current executable path")?;

    // Spawn daemon process with --foreground so the child actually runs
    // the server instead of spawning yet another process.
    let child = Command::new(exe)
        .arg("daemon")
        .arg("start")
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn daemon process")?;

    tracing::debug!("Spawned daemon process with PID {}", child.id());

    Ok(())
}

/// Get the path to the daemon socket.
///
/// Returns `/tmp/ty-find-{uid}.sock` on Unix systems where {uid} is the
/// current user ID. This ensures each user has their own daemon instance.
#[allow(unsafe_code)]
#[allow(clippy::unnecessary_wraps)] // Returns Err on non-Unix platforms
pub fn get_socket_path() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        // SAFETY: `libc::getuid()` is a simple syscall that returns the real
        // user ID. It has no preconditions and cannot cause UB.
        let uid = unsafe { libc::getuid() };
        let socket_name = format!("ty-find-{uid}.sock");
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
        let socket_path = get_socket_path().expect("should return a valid socket path");
        assert!(socket_path.to_string_lossy().contains("ty-find"));
        assert!(socket_path.to_string_lossy().ends_with(".sock"));
    }

    #[test]
    #[allow(unsafe_code)]
    fn test_socket_path_contains_uid() {
        #[cfg(unix)]
        {
            // SAFETY: `libc::getuid()` is a simple syscall with no preconditions.
            let uid = unsafe { libc::getuid() };
            let socket_path = get_socket_path().expect("should return a valid socket path");
            let path_str = socket_path.to_string_lossy();
            assert!(path_str.contains(&uid.to_string()));
        }
    }

    #[test]
    fn test_daemon_request_creation() {
        let params = HoverParams {
            workspace: PathBuf::from("/workspace"),
            file: PathBuf::from("file.py"),
            line: 10,
            column: 5,
        };

        let params_value = serde_json::to_value(params).expect("should serialize params");
        let request = DaemonRequest::new(Method::Hover, params_value);

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, Method::Hover);
        assert!(request.id > 0);
    }

    #[test]
    fn test_response_parsing() {
        let response_json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hover": null
            }
        }"#;

        let response: DaemonResponse =
            serde_json::from_str(response_json).expect("should parse response");
        assert!(response.is_success());
        assert!(!response.is_error());
        assert_eq!(response.id, 1);
    }

    #[test]
    fn test_error_response_parsing() {
        let response_json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32000,
                "message": "File not found",
                "data": {"file": "/path/to/file.py"}
            }
        }"#;

        let response: DaemonResponse =
            serde_json::from_str(response_json).expect("should parse error response");
        assert!(response.is_error());
        assert!(!response.is_success());

        let error = response.error.expect("should have error field");
        assert_eq!(error.code, -32000);
        assert_eq!(error.message, "File not found");
    }
}
