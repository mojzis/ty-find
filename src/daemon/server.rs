//! Daemon server implementation for persistent LSP connections.
//!
//! This module provides the main daemon server that listens on a Unix socket
//! and handles JSON-RPC requests from CLI clients. The server maintains a pool
//! of LSP clients and routes requests to the appropriate LSP server.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use anyhow::{Context, Result};
use serde_json::Value;

use crate::daemon::pool::LspClientPool;
use crate::daemon::protocol::*;

/// The daemon server that handles client connections and LSP requests.
///
/// The server listens on a Unix socket and processes JSON-RPC requests from
/// CLI clients. It maintains a pool of LSP clients (one per workspace) to
/// enable fast response times by reusing connections.
///
/// # Architecture
///
/// - One daemon per user (socket at `/tmp/ty-find-{uid}.sock`)
/// - One LSP client per workspace
/// - Idle timeout after 5 minutes of inactivity
/// - Graceful shutdown on SIGTERM or explicit shutdown request
pub struct DaemonServer {
    /// Path to the Unix socket
    socket_path: PathBuf,

    /// Pool of LSP clients (one per workspace)
    lsp_pool: Arc<Mutex<LspClientPool>>,

    /// Broadcast channel for shutdown signal
    shutdown_tx: broadcast::Sender<()>,

    /// Time when the daemon started
    start_time: Instant,
}

impl DaemonServer {
    /// Create a new daemon server with the specified socket path.
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path to the Unix socket file
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use ty_find::daemon::server::DaemonServer;
    ///
    /// let socket_path = PathBuf::from("/tmp/ty-find-1000.sock");
    /// let server = DaemonServer::new(socket_path);
    /// ```
    pub fn new(socket_path: PathBuf) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            socket_path,
            lsp_pool: Arc::new(Mutex::new(LspClientPool::new())),
            shutdown_tx,
            start_time: Instant::now(),
        }
    }

    /// Get the socket path for the current user.
    ///
    /// The socket path is `/tmp/ty-find-{uid}.sock` where `{uid}` is the
    /// current user's ID. This ensures each user has their own daemon.
    ///
    /// # Returns
    ///
    /// The socket path for the current user
    ///
    /// # Example
    ///
    /// ```
    /// use ty_find::daemon::server::DaemonServer;
    ///
    /// let socket_path = DaemonServer::get_socket_path();
    /// println!("Socket path: {}", socket_path.display());
    /// ```
    pub fn get_socket_path() -> PathBuf {
        #[cfg(unix)]
        {
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/ty-find-{}.sock", uid))
        }

        #[cfg(not(unix))]
        {
            // Fallback for non-Unix systems (e.g., Windows)
            PathBuf::from("/tmp/ty-find.sock")
        }
    }

    /// Start the daemon server and listen for connections.
    ///
    /// This method binds to the Unix socket and enters the main event loop,
    /// accepting connections and spawning tasks to handle them. It also starts
    /// an idle timeout task that will shut down the daemon after 5 minutes of
    /// inactivity.
    ///
    /// # Returns
    ///
    /// Returns an error if the socket cannot be bound or if the server
    /// encounters a fatal error.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ty_find::daemon::server::DaemonServer;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let socket_path = DaemonServer::get_socket_path();
    /// let server = DaemonServer::new(socket_path);
    /// server.start().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start(self) -> Result<()> {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("Failed to remove existing socket file")?;
        }

        // Bind to Unix socket
        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind Unix socket")?;

        tracing::info!("Daemon listening on {}", self.socket_path.display());

        // Set socket permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.socket_path, permissions)
                .context("Failed to set socket permissions")?;
        }

        let server = Arc::new(self);

        // NOTE: Using LocalSet because the LSP client uses std::sync::Mutex
        // which is not Send across await points. This is a limitation of the
        // current LSP client implementation. Ideally, TyLspClient should be
        // updated to use tokio::sync::Mutex instead.
        let local = tokio::task::LocalSet::new();

        // Spawn idle timeout task
        let server_clone = Arc::clone(&server);
        local.spawn_local(async move {
            server_clone.idle_timeout_task().await;
        });

        // Main accept loop
        let server_clone = Arc::clone(&server);
        let accept_loop = local.run_until(async move {
            let mut shutdown_rx = server_clone.shutdown_tx.subscribe();

            loop {
                tokio::select! {
                    // Accept new connection
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _addr)) => {
                                let server_clone = Arc::clone(&server_clone);
                                tokio::task::spawn_local(async move {
                                    if let Err(_e) = server_clone.handle_connection(stream).await {
                                        tracing::error!("Connection error: {}", _e);
                                    }
                                });
                            }
                            Err(_e) => {
                                tracing::error!("Accept error: {}", _e);
                            }
                        }
                    }

                    // Shutdown signal
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Shutdown signal received");
                        break;
                    }
                }
            }
        });

        accept_loop.await;

        // Cleanup
        server.cleanup().await?;

        Ok(())
    }

    /// Handle a single client connection.
    ///
    /// Reads JSON-RPC requests from the client, processes them, and sends
    /// responses back. Handles multiple requests over a single connection.
    ///
    /// # Arguments
    ///
    /// * `stream` - The Unix socket stream for this connection
    async fn handle_connection(self: Arc<Self>, stream: UnixStream) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut buffer = String::new();

        loop {
            buffer.clear();

            // Read the request
            let bytes_read = reader.read_line(&mut buffer).await?;
            if bytes_read == 0 {
                // EOF - client disconnected
                break;
            }

            // Parse JSON-RPC request
            let request: DaemonRequest = match serde_json::from_str(&buffer) {
                Ok(req) => req,
                Err(_e) => {
                    let error_response = DaemonResponse::error(
                        0,
                        DaemonError::parse_error(),
                    );
                    let response_json = serde_json::to_string(&error_response)?;
                    writer.write_all(response_json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                    continue;
                }
            };

            tracing::debug!("Received request: {:?}", request.method);

            // Process the request
            let response = self.handle_request(request).await;

            // Send response
            let response_json = serde_json::to_string(&response)?;
            writer.write_all(response_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            tracing::debug!("Sent response for request ID {}", response.id);
        }

        Ok(())
    }

    /// Process a single JSON-RPC request and return a response.
    ///
    /// This method dispatches the request to the appropriate handler based on
    /// the method name.
    ///
    /// # Arguments
    ///
    /// * `request` - The JSON-RPC request to process
    ///
    /// # Returns
    ///
    /// A JSON-RPC response (either success or error)
    async fn handle_request(&self, request: DaemonRequest) -> DaemonResponse {
        let result = match request.method {
            Method::Hover => self.handle_hover(request.params).await,
            Method::Definition => self.handle_definition(request.params).await,
            Method::WorkspaceSymbols => self.handle_workspace_symbols(request.params).await,
            Method::DocumentSymbols => self.handle_document_symbols(request.params).await,
            Method::Diagnostics => self.handle_diagnostics(request.params).await,
            Method::Ping => self.handle_ping(request.params).await,
            Method::Shutdown => self.handle_shutdown(request.params).await,
        };

        match result {
            Ok(value) => DaemonResponse::success(request.id, value),
            Err(e) => DaemonResponse::error(request.id, DaemonError::internal_error(e.to_string())),
        }
    }

    /// Handle a hover request.
    async fn handle_hover(&self, params: Value) -> Result<Value> {
        let params: HoverParams = serde_json::from_value(params)
            .context("Invalid hover parameters")?;

        let client = {
            self.lsp_pool.lock().await.get_or_create(params.workspace).await?
        };

        let file_str = params.file.to_string_lossy().to_string();
        let hover = client.hover(&file_str, params.line, params.column).await?;

        let result = HoverResult { hover };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a definition request.
    async fn handle_definition(&self, params: Value) -> Result<Value> {
        let params: DefinitionParams = serde_json::from_value(params)
            .context("Invalid definition parameters")?;

        let client = {
            self.lsp_pool.lock().await.get_or_create(params.workspace).await?
        };

        let file_str = params.file.to_string_lossy().to_string();
        let locations = client.goto_definition(&file_str, params.line, params.column).await?;

        let location = locations.into_iter().next();
        let result = DefinitionResult { location };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a workspace symbols request.
    async fn handle_workspace_symbols(&self, params: Value) -> Result<Value> {
        let params: WorkspaceSymbolsParams = serde_json::from_value(params)
            .context("Invalid workspace symbols parameters")?;

        let client = {
            self.lsp_pool.lock().await.get_or_create(params.workspace).await?
        };

        let mut symbols = client.workspace_symbols(&params.query).await?;

        // Apply limit if specified
        if let Some(limit) = params.limit {
            symbols.truncate(limit);
        }

        let result = WorkspaceSymbolsResult { symbols };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a document symbols request.
    async fn handle_document_symbols(&self, params: Value) -> Result<Value> {
        let params: DocumentSymbolsParams = serde_json::from_value(params)
            .context("Invalid document symbols parameters")?;

        let client = {
            self.lsp_pool.lock().await.get_or_create(params.workspace).await?
        };

        let file_str = params.file.to_string_lossy().to_string();
        let symbols = client.document_symbols(&file_str).await?;

        let result = DocumentSymbolsResult { symbols };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a diagnostics request.
    async fn handle_diagnostics(&self, _params: Value) -> Result<Value> {
        // Diagnostics are not yet implemented in the LSP client
        // Return empty diagnostics for now
        let result = DiagnosticsResult {
            diagnostics: vec![],
        };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a ping request.
    async fn handle_ping(&self, _params: Value) -> Result<Value> {
        let pool = self.lsp_pool.lock().await;
        let active_workspaces = pool.len();
        drop(pool);

        let result = PingResult {
            status: "running".to_string(),
            uptime: self.start_time.elapsed().as_secs(),
            active_workspaces,
            cache_size: 0, // Cache not yet implemented
        };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a shutdown request.
    async fn handle_shutdown(&self, _params: Value) -> Result<Value> {
        tracing::info!("Shutdown requested");

        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        let result = ShutdownResult {
            message: "Daemon shutting down".to_string(),
        };
        Ok(serde_json::to_value(result)?)
    }

    /// Idle timeout task that shuts down the daemon after inactivity.
    ///
    /// Checks every minute and shuts down if there's been no activity for 5 minutes.
    async fn idle_timeout_task(&self) {
        let idle_timeout = Duration::from_secs(300); // 5 minutes
        let check_interval = Duration::from_secs(60); // 1 minute

        loop {
            tokio::time::sleep(check_interval).await;

            // Clean up idle LSP clients
            let pool = self.lsp_pool.lock().await;
            let removed = pool.cleanup_idle(idle_timeout);
            if removed > 0 {
                tracing::info!("Removed {} idle LSP clients", removed);
            }

            // Check if daemon should shut down (all clients idle)
            if pool.is_empty() && self.start_time.elapsed() > idle_timeout {
                tracing::info!("Daemon idle timeout, shutting down");
                let _ = self.shutdown_tx.send(());
                break;
            }
            drop(pool);
        }
    }

    /// Graceful shutdown cleanup.
    ///
    /// Removes the socket file and releases resources.
    async fn cleanup(&self) -> Result<()> {
        tracing::info!("Cleaning up daemon resources");

        // Remove socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("Failed to remove socket file")?;
        }

        Ok(())
    }

    /// Spawn the daemon as a background process.
    ///
    /// This method forks the current process (on Unix) and starts the daemon
    /// server in the background. The parent process returns immediately.
    ///
    /// # Returns
    ///
    /// Returns an error if the daemon cannot be spawned.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ty_find::daemon::server::DaemonServer;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// DaemonServer::spawn_background()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn spawn_background() -> Result<()> {
        let socket_path = Self::get_socket_path();

        // Check if daemon is already running
        if socket_path.exists() {
            tracing::debug!("Daemon socket already exists, assuming daemon is running");
            return Ok(());
        }

        tracing::info!("Spawning daemon in background");

        // Spawn a new process to run the daemon
        #[cfg(unix)]
        {
            use std::process::Command;

            // Get the current executable path
            let exe = std::env::current_exe()
                .context("Failed to get current executable path")?;

            // Spawn daemon process in background
            Command::new(exe)
                .arg("daemon")
                .arg("start")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .context("Failed to spawn daemon process")?;

            // Wait a bit for daemon to start
            std::thread::sleep(Duration::from_millis(500));
        }

        #[cfg(not(unix))]
        {
            anyhow::bail!("Background daemon spawning is not supported on this platform");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_socket_path() {
        let path = DaemonServer::get_socket_path();
        assert!(path.to_string_lossy().contains("ty-find"));
    }

    #[test]
    fn test_server_creation() {
        let socket_path = PathBuf::from("/tmp/test-ty-find.sock");
        let server = DaemonServer::new(socket_path.clone());
        assert_eq!(server.socket_path, socket_path);
    }

    #[tokio::test]
    async fn test_ping_handler() {
        let socket_path = PathBuf::from("/tmp/test-ty-find.sock");
        let server = DaemonServer::new(socket_path);

        let params = serde_json::json!({});
        let result = server.handle_ping(params).await;

        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.get("status").is_some());
        assert!(value.get("uptime").is_some());
    }
}
