//! Daemon client for communicating with the persistent tyf daemon.
//!
//! This module provides a client that connects to the daemon using a dual
//! transport strategy: Unix domain socket (primary) with TCP fallback for
//! sandboxed environments. The transport is auto-negotiated with zero
//! configuration.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixStream};
use tokio::time::timeout;

use super::pidfile::{self, PidfileData};
use crate::debug::DebugLog;

use super::protocol::{
    BatchReferencesParams, BatchReferencesQuery, BatchReferencesResult, CallDirection,
    CallHierarchyParams, CallHierarchyQuery, CallHierarchyResult, DaemonRequest, DaemonResponse,
    DefinitionParams, DefinitionResult, DocumentSymbolsParams, DocumentSymbolsResult, HoverParams,
    HoverResult, InspectParams, InspectResult, MembersParams, MembersResult, Method, PingParams,
    PingResult, ReferencesParams, ReferencesResult, ShutdownParams, ShutdownResult,
    UnsupportedByTy, WorkspaceSymbolsParams, WorkspaceSymbolsResult,
};

/// Default timeout for daemon operations (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for daemon startup (2 seconds).
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum number of startup retry attempts.
const MAX_STARTUP_RETRIES: usize = 20;

/// Delay between startup retry attempts (100ms).
const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Transport layer abstraction — both `AsyncRead` and `AsyncWrite`.
///
/// Object-safe supertrait alias so we can store `Box<dyn DaemonTransport>`.
trait DaemonTransport: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> DaemonTransport for T {}

/// Client for communicating with the tyf daemon.
///
/// The client connects to the daemon via Unix domain socket (primary) or TCP
/// (fallback), and sends JSON-RPC 2.0 requests. Messages are framed using
/// Content-Length headers similar to the LSP protocol.
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
    /// Connection to the daemon (Unix socket or TCP stream).
    stream: Box<dyn DaemonTransport>,

    /// Timeout for daemon operations.
    timeout: Duration,

    /// Optional debug log for tracing RPC requests/responses.
    debug_log: Option<Arc<DebugLog>>,
}

impl DaemonClient {
    /// Connect to an existing daemon with the default timeout.
    ///
    /// Tries Unix socket first, then falls back to TCP if the Unix connect
    /// fails with `EPERM` (sandbox), `ECONNREFUSED`, or `ENOENT`.
    pub async fn connect() -> Result<Self> {
        Self::connect_with_timeout(DEFAULT_TIMEOUT).await
    }

    /// Connect to an existing daemon with a custom timeout.
    ///
    /// Connection strategy:
    /// 1. Read pidfile to get socket path and TCP port.
    /// 2. Try `connect()` to the Unix socket.
    /// 3. If Unix fails → fall back to TCP `127.0.0.1:{tcp_port}`.
    /// 4. If neither works → return error.
    pub async fn connect_with_timeout(timeout: Duration) -> Result<Self> {
        let pidfile_path = pidfile::get_pidfile_path()?;

        // Try pidfile-based connection first (new format)
        if pidfile_path.exists() {
            if let Ok(data) = PidfileData::read(&pidfile_path) {
                return Self::connect_with_pidfile(&data, timeout).await;
            }
            tracing::debug!("Pidfile exists but unreadable, falling back to socket path");
        }

        // Fallback: try connecting directly to the socket path (backward
        // compat with old daemon that doesn't write a pidfile)
        let socket_path = get_socket_path()?;
        let stream = UnixStream::connect(&socket_path)
            .await
            .context("Failed to connect to daemon (no pidfile, socket connect failed)")?;

        tracing::debug!("Connected to daemon via Unix socket (legacy, no pidfile)");

        Ok(Self { stream: Box::new(stream), timeout, debug_log: None })
    }

    /// Connect using pidfile data: try Unix socket first, TCP fallback.
    async fn connect_with_pidfile(data: &PidfileData, timeout: Duration) -> Result<Self> {
        // Try Unix socket first (fast path)
        match UnixStream::connect(&data.socket).await {
            Ok(stream) => {
                tracing::debug!("Connected to daemon via Unix socket");
                return Ok(Self { stream: Box::new(stream), timeout, debug_log: None });
            }
            Err(e) => {
                // EPERM (sandbox), ECONNREFUSED, or ENOENT → fall back to TCP.
                // No timeout needed — sandbox-blocked connect returns EPERM immediately.
                tracing::debug!("Unix socket connect failed ({e}), trying TCP fallback");
            }
        }

        // TCP fallback
        let addr = format!("127.0.0.1:{}", data.tcp_port);
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("TCP fallback to {addr} also failed"))?;

        tracing::info!("Connected to daemon via TCP fallback ({addr})");

        Ok(Self { stream: Box::new(stream), timeout, debug_log: None })
    }

    /// Attach a debug log for tracing RPC requests and responses.
    pub fn set_debug_log(&mut self, log: Arc<DebugLog>) {
        self.debug_log = Some(log);
    }

    /// Send a JSON-RPC request to the daemon and wait for response.
    pub async fn send_request(&mut self, method: Method, params: Value) -> Result<DaemonResponse> {
        let mut request = DaemonRequest::new(method, params);
        // Set debug flag so the daemon includes raw LSP trace in the response
        request.debug = self.debug_log.is_some();

        // Serialize request to JSON
        let request_json =
            serde_json::to_string(&request).context("Failed to serialize request")?;

        // Log the outgoing RPC request
        if let Some(ref log) = self.debug_log {
            let params_json = serde_json::to_string_pretty(&request.params).unwrap_or_default();
            log.log_rpc_request(method.as_str(), &params_json);
        }

        let rpc_start = Instant::now();

        // Frame with Content-Length header
        let message = format!("Content-Length: {}\r\n\r\n{request_json}", request_json.len());

        // Send request with timeout
        let response = timeout(self.timeout, async {
            self.stream
                .write_all(message.as_bytes())
                .await
                .context("Failed to write request to daemon")?;

            tracing::debug!("Sent request: method={}", method.as_str());

            // Read response
            self.read_response().await
        })
        .await
        .context("Request timed out")??;

        // Log the incoming RPC response
        if let Some(ref log) = self.debug_log {
            let elapsed_ms = rpc_start.elapsed().as_millis();
            let response_json = serde_json::to_string_pretty(&response).unwrap_or_default();
            log.log_rpc_response(elapsed_ms, response.is_success(), &response_json);

            // Log daemon-side LSP trace if available
            if let Some(ref trace) = response.debug_trace {
                log.log_lsp_trace(
                    &trace.method,
                    &serde_json::to_string_pretty(&trace.params).unwrap_or_default(),
                    &serde_json::to_string_pretty(&trace.response).unwrap_or_default(),
                );
            }
        }

        Ok(response)
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
        let params = WorkspaceSymbolsParams {
            workspace,
            query,
            limit: None,
            exact_name: None,
            container_name: None,
        };
        self.execute(Method::WorkspaceSymbols, params).await
    }

    /// Execute a workspace symbols request filtered to exact name matches.
    pub async fn execute_workspace_symbols_exact(
        &mut self,
        workspace: PathBuf,
        query: String,
    ) -> Result<WorkspaceSymbolsResult> {
        let exact_name = Some(query.clone());
        let params = WorkspaceSymbolsParams {
            workspace,
            query,
            limit: None,
            exact_name,
            container_name: None,
        };
        self.execute(Method::WorkspaceSymbols, params).await
    }

    /// Execute a workspace symbols request filtered to exact name + container.
    ///
    /// Used for dotted notation like `Class.method`: searches for `symbol_name`
    /// and filters results where `container_name` matches `container`.
    pub async fn execute_workspace_symbols_exact_with_container(
        &mut self,
        workspace: PathBuf,
        symbol_name: String,
        container: String,
    ) -> Result<WorkspaceSymbolsResult> {
        let params = WorkspaceSymbolsParams {
            workspace,
            query: symbol_name.clone(),
            limit: None,
            exact_name: Some(symbol_name),
            container_name: Some(container),
        };
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

    /// Execute a batch references request (multiple queries in one RPC call).
    pub async fn execute_batch_references(
        &mut self,
        workspace: PathBuf,
        queries: Vec<BatchReferencesQuery>,
        include_declaration: bool,
    ) -> Result<BatchReferencesResult> {
        let params = BatchReferencesParams { workspace, queries, include_declaration };
        self.execute(Method::BatchReferences, params).await
    }

    /// Execute an inspect request (hover, and optionally references, in one call).
    pub async fn execute_inspect(
        &mut self,
        workspace: PathBuf,
        file: String,
        line: u32,
        column: u32,
        include_references: bool,
    ) -> Result<InspectResult> {
        let params = InspectParams {
            workspace,
            file: PathBuf::from(file),
            line,
            column,
            include_references,
        };
        self.execute(Method::Inspect, params).await
    }

    /// Execute a members request (class members with type signatures).
    pub async fn execute_members(
        &mut self,
        workspace: PathBuf,
        file: String,
        class_name: String,
        include_all: bool,
    ) -> Result<MembersResult> {
        let params =
            MembersParams { workspace, file: PathBuf::from(file), class_name, include_all };
        self.execute(Method::Members, params).await
    }

    /// Execute a call-hierarchy request: the daemon walks the whole tree and
    /// returns it finished.
    ///
    /// Unlike the other `execute_*` helpers this does not go through
    /// [`Self::execute`], because the `unsupported_by_ty` error must survive as
    /// a typed error rather than collapsing into a generic daemon-error string
    /// — the CLI needs the `ty` version out of it.
    pub async fn execute_call_hierarchy(
        &mut self,
        workspace: PathBuf,
        queries: Vec<CallHierarchyQuery>,
        direction: CallDirection,
        depth: u32,
        include_external: bool,
    ) -> Result<CallHierarchyResult> {
        let params = CallHierarchyParams { workspace, queries, direction, depth, include_external };
        let params_value =
            serde_json::to_value(params).context("Failed to serialize call_hierarchy params")?;

        let response = self.send_request(Method::CallHierarchy, params_value).await?;

        if let Some(error) = response.error {
            if let Some((feature, ty_version)) = error.as_unsupported_by_ty() {
                return Err(UnsupportedByTy::from_slug(feature, ty_version).into());
            }
            anyhow::bail!("Daemon error: {}", error.message);
        }

        let result = response.result.context("Response missing result field")?;
        serde_json::from_value(result).context("Failed to deserialize call_hierarchy result")
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

/// Version of the current binary, used to detect stale daemons after upgrades.
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serializes daemon startup within one process.
///
/// The CLI runs one command per process, so check-then-spawn was safe there.
/// The MCP frontend serves many tool calls from a single process and rmcp runs
/// each in its own task, so without this gate concurrent first calls each see
/// "no daemon", each spawn one, and each new daemon unlinks the previous
/// winner's socket on bind — leaving orphaned daemons (and their `ty lsp`
/// children) running until their idle timeout.
///
/// A `tokio::sync::Mutex` rather than a `std` one: it is held across the
/// spawn-and-wait `.await`s by design.
static STARTUP_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Ensure the daemon is running, starting it if necessary.
///
/// If an existing daemon is running but was built from a different version of
/// the binary (e.g. after `pip install --upgrade`), it is shut down and a fresh
/// one is spawned so the user always talks to a daemon matching their CLI.
///
/// Concurrent callers in one process are serialized: the first performs the
/// check and any spawn, the rest wait and then find a running daemon.
pub async fn ensure_daemon_running() -> Result<()> {
    let _startup_guard = STARTUP_GATE.lock().await;
    let socket_path = get_socket_path()?;
    let pidfile_path = pidfile::get_pidfile_path()?;

    // Check if daemon is reachable (via pidfile or socket)
    let reachable = pidfile_path.exists() || socket_path.exists();

    if reachable {
        match DaemonClient::connect().await {
            Ok(mut client) => {
                // Verify the running daemon has the same version as this binary.
                match client.ping().await {
                    Ok(ping) if ping.version == CLIENT_VERSION => {
                        tracing::debug!("Daemon already running (v{})", ping.version);
                        return Ok(());
                    }
                    Ok(ping) => {
                        tracing::warn!(
                            "Daemon version mismatch: daemon v{}, client v{} — restarting",
                            ping.version,
                            CLIENT_VERSION,
                        );
                        // Best-effort shutdown; ignore errors (e.g. if it already exited).
                        let _ = client.shutdown().await;
                        // Give the old daemon a moment to release the socket.
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        let _ = std::fs::remove_file(&socket_path);
                        let _ = std::fs::remove_file(&pidfile_path);
                    }
                    Err(e) => {
                        tracing::warn!("Ping failed on existing daemon: {e} — restarting");
                        let _ = client.shutdown().await;
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        let _ = std::fs::remove_file(&socket_path);
                        let _ = std::fs::remove_file(&pidfile_path);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Daemon unreachable: {e}");
                // Try to clean up stale files
                let _ = std::fs::remove_file(&socket_path);
                let _ = std::fs::remove_file(&pidfile_path);
            }
        }
    }

    // Spawn daemon in background
    tracing::info!("Starting daemon...");
    spawn_daemon()?;

    // Wait for daemon to start — check for pidfile (new) or socket (legacy)
    for i in 0..MAX_STARTUP_RETRIES {
        tokio::time::sleep(STARTUP_RETRY_DELAY).await;

        let ready = pidfile_path.exists() || socket_path.exists();
        if ready {
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
    fn test_client_version_matches_cargo_pkg() {
        // CLIENT_VERSION should be the same as Cargo.toml version at compile time
        assert!(!CLIENT_VERSION.is_empty());
        assert_eq!(CLIENT_VERSION, env!("CARGO_PKG_VERSION"));
    }

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

    #[tokio::test]
    async fn test_connect_with_pidfile_tcp_fallback() {
        // Spin up a TCP listener that speaks the daemon protocol
        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind should succeed");
        let port = listener.local_addr().expect("addr").port();

        // Spawn a task that accepts one connection and responds to a ping
        let handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf_reader = tokio::io::BufReader::new(&mut stream);

            // Read request
            let mut header = String::new();
            buf_reader.read_line(&mut header).await.expect("read header");
            let len: usize = header
                .trim()
                .strip_prefix("Content-Length: ")
                .expect("header")
                .parse()
                .expect("parse");
            let mut empty = String::new();
            buf_reader.read_line(&mut empty).await.expect("read sep");
            let mut body = vec![0u8; len];
            buf_reader.read_exact(&mut body).await.expect("read body");

            // Send a ping response
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "status": "running",
                    "version": env!("CARGO_PKG_VERSION"),
                    "uptime": 1,
                    "active_workspaces": 0,
                    "cache_size": 0,
                    "socket_path": "/tmp/nonexistent.sock",
                    "tcp_port": port
                }
            });
            let resp_str = serde_json::to_string(&resp).expect("serialize");
            let framed = format!("Content-Length: {}\r\n\r\n{resp_str}", resp_str.len());
            stream.write_all(framed.as_bytes()).await.expect("write");
            stream.flush().await.expect("flush");
        });

        // Create a pidfile pointing to a nonexistent socket but valid TCP port
        let data = PidfileData {
            pid: std::process::id(),
            socket: PathBuf::from("/tmp/nonexistent-ty-find-test.sock"),
            tcp_port: port,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Try connecting — Unix socket should fail, TCP should succeed
        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let ping = client.ping().await.expect("ping should succeed");
        assert_eq!(ping.status, "running");

        handle.await.expect("server task");
    }

    /// Helper: spin up a TCP listener that answers one request with `response`
    /// and hand back a pidfile pointing at it.
    ///
    /// Used to exercise wire-level error handling without a real daemon or a
    /// real `ty` — in particular the `unsupported_by_ty` path, which would
    /// otherwise need an old `ty` installed in CI.
    async fn spawn_daemon_returning(
        response: serde_json::Value,
    ) -> (tokio::task::JoinHandle<()>, PidfileData) {
        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind should succeed");
        let port = listener.local_addr().expect("addr").port();

        let handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf_reader = tokio::io::BufReader::new(&mut stream);

            let mut header = String::new();
            buf_reader.read_line(&mut header).await.expect("read header");
            let len: usize = header
                .trim()
                .strip_prefix("Content-Length: ")
                .expect("header")
                .parse()
                .expect("parse");
            let mut empty = String::new();
            buf_reader.read_line(&mut empty).await.expect("read sep");
            let mut body = vec![0u8; len];
            buf_reader.read_exact(&mut body).await.expect("read body");

            let request: serde_json::Value =
                serde_json::from_slice(&body).expect("request should be JSON");
            let mut resp = response;
            resp["id"] = request["id"].clone();

            let resp_str = serde_json::to_string(&resp).expect("serialize");
            let framed = format!("Content-Length: {}\r\n\r\n{resp_str}", resp_str.len());
            stream.write_all(framed.as_bytes()).await.expect("write");
            stream.flush().await.expect("flush");
        });

        let data = PidfileData {
            pid: std::process::id(),
            socket: PathBuf::from("/tmp/nonexistent-ty-find-caps-test.sock"),
            tcp_port: port,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        (handle, data)
    }

    fn sample_call_query() -> CallHierarchyQuery {
        CallHierarchyQuery {
            label: "process_order".to_string(),
            file: PathBuf::from("orders.py"),
            line: 40,
            column: 4,
        }
    }

    /// A daemon that reports the capability missing must surface as the typed
    /// [`UnsupportedByTy`] error carrying the ty version — not as a generic
    /// "Daemon error" string, and not as an empty tree (which would be
    /// indistinguishable from "this function calls nothing").
    #[tokio::test]
    async fn call_hierarchy_unsupported_surfaces_typed_error_with_version() {
        let (handle, data) = spawn_daemon_returning(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32005,
                "message": "unsupported_by_ty",
                "data": { "feature": "call_hierarchy", "ty_version": "0.0.18" }
            }
        }))
        .await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let err = client
            .execute_call_hierarchy(
                PathBuf::from("/workspace"),
                vec![sample_call_query()],
                CallDirection::Outgoing,
                2,
                false,
            )
            .await
            .expect_err("missing capability must be an error");

        let unsupported =
            err.downcast_ref::<UnsupportedByTy>().expect("should be a typed UnsupportedByTy error");
        assert_eq!(unsupported.ty_version, "0.0.18");
        assert_eq!(unsupported.feature, "call hierarchy");
        assert_eq!(unsupported.min_version, "0.0.41");

        let rendered = unsupported.to_string();
        assert!(rendered.contains("0.0.18"), "message must name the installed version: {rendered}");
        assert!(rendered.contains("0.0.41"), "message must name the required version: {rendered}");

        handle.await.expect("server task");
    }

    /// A slug this CLI does not recognise (a newer daemon gating a feature it
    /// has not heard of) must still render a usable message rather than being
    /// dropped or rendered blank.
    #[tokio::test]
    async fn call_hierarchy_unknown_feature_slug_still_renders() {
        let (handle, data) = spawn_daemon_returning(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32005,
                "message": "Feature not supported by the installed ty",
                "data": { "feature": "type_hierarchy", "ty_version": "0.0.30" }
            }
        }))
        .await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let err = client
            .execute_call_hierarchy(
                PathBuf::from("/workspace"),
                vec![sample_call_query()],
                CallDirection::Outgoing,
                2,
                false,
            )
            .await
            .expect_err("missing capability must be an error");

        let unsupported = err.downcast_ref::<UnsupportedByTy>().expect("still a typed error");
        assert_eq!(unsupported.feature, "type_hierarchy");
        let rendered = unsupported.to_string();
        assert!(rendered.contains("type_hierarchy"), "got: {rendered}");
        assert!(rendered.contains("0.0.30"), "got: {rendered}");

        handle.await.expect("server task");
    }

    /// The wire message is a sentence, not the machine slug: a caller that does
    /// not special-case -32005 prints `message` verbatim.
    #[test]
    fn unsupported_by_ty_error_message_is_human_readable() {
        let error =
            crate::daemon::protocol::DaemonError::unsupported_by_ty("call_hierarchy", "0.0.18");
        assert_eq!(error.code, -32005);
        assert_eq!(error.message, "Feature not supported by the installed ty");
        assert_eq!(error.as_unsupported_by_ty(), Some(("call_hierarchy", "0.0.18")));
    }

    /// Any other error code is not an unsupported-capability error.
    #[test]
    fn other_error_codes_are_not_unsupported_by_ty() {
        let error = crate::daemon::protocol::DaemonError::symbol_not_found("foo");
        assert_eq!(error.as_unsupported_by_ty(), None);
    }

    /// Any other daemon error keeps the generic path — the capability gate must
    /// not swallow unrelated failures.
    #[tokio::test]
    async fn call_hierarchy_other_errors_are_not_treated_as_unsupported() {
        let (handle, data) = spawn_daemon_returning(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32603, "message": "Internal error: boom" }
        }))
        .await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let err = client
            .execute_call_hierarchy(
                PathBuf::from("/workspace"),
                vec![sample_call_query()],
                CallDirection::Outgoing,
                2,
                false,
            )
            .await
            .expect_err("internal error must propagate");

        assert!(err.downcast_ref::<UnsupportedByTy>().is_none());
        assert!(err.to_string().contains("boom"), "got: {err}");

        handle.await.expect("server task");
    }

    /// A successful walk deserializes into the tree the formatter renders,
    /// markers included.
    #[tokio::test]
    async fn call_hierarchy_success_deserializes_tree_with_markers() {
        let (handle, data) = spawn_daemon_returning(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "direction": "outgoing",
                "depth": 2,
                "entries": [{
                    "label": "process_order",
                    "roots": [{
                        "name": "process_order", "kind": 12, "detail": "orders",
                        "uri": "file:///w/orders.py", "line": 40, "column": 4,
                        "external": false, "cycle": false, "deduped": false,
                        "children": [
                            {
                                "name": "check_inventory", "kind": 12, "detail": "inventory",
                                "uri": "file:///w/inventory.py", "line": 11, "column": 4,
                                "call_sites": [42],
                                "external": false, "cycle": false, "deduped": true
                            },
                            {
                                "name": "floor", "kind": 12, "detail": "math",
                                "external": true, "cycle": false, "deduped": false
                            }
                        ]
                    }]
                }]
            }
        }))
        .await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let result = client
            .execute_call_hierarchy(
                PathBuf::from("/workspace"),
                vec![sample_call_query()],
                CallDirection::Outgoing,
                2,
                false,
            )
            .await
            .expect("walk should succeed");

        assert_eq!(result.direction, CallDirection::Outgoing);
        assert_eq!(result.depth, 2);
        assert_eq!(result.entries.len(), 1);

        let root = &result.entries[0].roots[0];
        assert_eq!(root.name, "process_order");
        assert_eq!(root.children.len(), 2);
        assert!(root.children[0].deduped, "dedup flag must survive the wire");
        assert_eq!(root.children[0].call_sites, vec![42]);
        assert!(root.children[1].external);
        assert!(
            root.children[1].uri.is_none(),
            "an external node carries no location unless --external"
        );
        assert!(
            root.children[1].line.is_none() && root.children[1].column.is_none(),
            "and no position either \u{2014} coordinates with no file resolve to nothing"
        );

        handle.await.expect("server task");
    }

    /// Helper: spin up a TCP listener that responds to one ping with the given version.
    async fn spawn_fake_daemon(version: &str) -> (tokio::task::JoinHandle<()>, PidfileData) {
        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind should succeed");
        let port = listener.local_addr().expect("addr").port();
        let version = version.to_string();
        let pidfile_version = version.clone();

        let handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf_reader = tokio::io::BufReader::new(&mut stream);

            // Read request
            let mut header = String::new();
            buf_reader.read_line(&mut header).await.expect("read header");
            let len: usize = header
                .trim()
                .strip_prefix("Content-Length: ")
                .expect("header")
                .parse()
                .expect("parse");
            let mut empty = String::new();
            buf_reader.read_line(&mut empty).await.expect("read sep");
            let mut body = vec![0u8; len];
            buf_reader.read_exact(&mut body).await.expect("read body");

            // Send a ping response with the specified version
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "status": "running",
                    "version": version,
                    "uptime": 100,
                    "active_workspaces": 1,
                    "cache_size": 0,
                    "socket_path": "/tmp/nonexistent.sock",
                    "tcp_port": port,
                    "pid": 99999
                }
            });
            let resp_str = serde_json::to_string(&resp).expect("serialize");
            let framed = format!("Content-Length: {}\r\n\r\n{resp_str}", resp_str.len());
            stream.write_all(framed.as_bytes()).await.expect("write");
            stream.flush().await.expect("flush");
        });

        let data = PidfileData {
            pid: std::process::id(),
            socket: PathBuf::from("/tmp/nonexistent-ty-find-version-test.sock"),
            tcp_port: port,
            version: pidfile_version,
        };

        (handle, data)
    }

    #[tokio::test]
    async fn test_version_mismatch_detected() {
        let (handle, data) = spawn_fake_daemon("0.0.1-old").await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect via TCP fallback");

        let ping = client.ping().await.expect("ping should succeed");
        assert_eq!(ping.version, "0.0.1-old");
        assert_ne!(ping.version, CLIENT_VERSION, "versions should differ");

        handle.await.expect("server task");
    }

    #[tokio::test]
    async fn test_version_match_detected() {
        let (handle, data) = spawn_fake_daemon(CLIENT_VERSION).await;

        let mut client = DaemonClient::connect_with_pidfile(&data, DEFAULT_TIMEOUT)
            .await
            .expect("should connect");

        let ping = client.ping().await.expect("ping should succeed");
        assert_eq!(ping.version, CLIENT_VERSION, "versions should match");

        handle.await.expect("server task");
    }
}
