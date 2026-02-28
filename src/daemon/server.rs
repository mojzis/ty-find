//! Daemon server implementation for persistent LSP connections.
//!
//! This module provides the main daemon server that listens on a Unix socket
//! and handles JSON-RPC requests from CLI clients. The server maintains a pool
//! of LSP clients and routes requests to the appropriate LSP server.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};

use crate::daemon::pool::LspClientPool;
use crate::daemon::protocol::{
    BatchReferencesEntry, BatchReferencesParams, BatchReferencesResult, DaemonError, DaemonRequest,
    DaemonResponse, DefinitionParams, DefinitionResult, DiagnosticsResult, DocumentSymbolsParams,
    DocumentSymbolsResult, HoverParams, HoverResult, InspectParams, InspectResult, Method,
    PingResult, ReferencesParams, ReferencesResult, ShutdownResult, WorkspaceSymbolsParams,
    WorkspaceSymbolsResult,
};
use crate::lsp::client::TyLspClient;
use crate::lsp::protocol::Hover;

/// The daemon server that handles client connections and LSP requests.
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
    /// Delegates to the canonical implementation in [`super::client::get_socket_path`].
    pub fn get_socket_path() -> PathBuf {
        super::client::get_socket_path().expect("Failed to determine socket path (non-Unix?)")
    }

    /// Start the daemon server and listen for connections.
    pub async fn start(self) -> Result<()> {
        // Remove existing socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("Failed to remove existing socket file")?;
        }

        // Bind to Unix socket
        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;

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

        // NOTE: Using LocalSet because LspClientPool uses std::sync::Mutex
        // internally and spawn_local avoids Send requirements. TyLspClient
        // itself is now Send (stdin uses tokio::sync::Mutex), but the pool's
        // internal locking pattern is simpler with LocalSet.
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
                                let server_for_conn = Arc::clone(&server_clone);
                                tokio::task::spawn_local(async move {
                                    if let Err(err) = server_for_conn.handle_connection(stream).await {
                                        tracing::error!("Connection error: {err}");
                                    }
                                });
                            }
                            Err(err) => {
                                tracing::error!("Accept error: {err}");
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
    async fn handle_connection(self: Arc<Self>, stream: UnixStream) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut header_line = String::new();

        loop {
            header_line.clear();

            // Read Content-Length header
            let bytes_read = reader.read_line(&mut header_line).await?;
            if bytes_read == 0 {
                // EOF - client disconnected
                break;
            }

            // Parse content length
            let content_length =
                if let Some(len_str) = header_line.trim().strip_prefix("Content-Length: ") {
                    if let Ok(len) = len_str.parse::<usize>() {
                        len
                    } else {
                        send_error_response(&mut writer, DaemonError::parse_error()).await?;
                        continue;
                    }
                } else {
                    send_error_response(&mut writer, DaemonError::parse_error()).await?;
                    continue;
                };

            // Read empty separator line
            let mut empty_line = String::new();
            reader.read_line(&mut empty_line).await?;

            // Read request body
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).await?;

            // Parse JSON-RPC request
            let Ok(request) = serde_json::from_slice::<DaemonRequest>(&body) else {
                send_error_response(&mut writer, DaemonError::parse_error()).await?;
                continue;
            };

            tracing::debug!("Received request: {:?}", request.method);

            // Process the request
            let response = self.handle_request(request).await;

            // Send response with Content-Length framing
            let response_json = serde_json::to_string(&response)?;
            let framed = format!("Content-Length: {}\r\n\r\n{response_json}", response_json.len());
            writer.write_all(framed.as_bytes()).await?;
            writer.flush().await?;

            tracing::debug!("Sent response for request ID {}", response.id);
        }

        Ok(())
    }

    /// Process a single JSON-RPC request and return a response.
    async fn handle_request(&self, request: DaemonRequest) -> DaemonResponse {
        let result = match request.method {
            Method::Hover => self.handle_hover(request.params).await,
            Method::Definition => self.handle_definition(request.params).await,
            Method::WorkspaceSymbols => self.handle_workspace_symbols(request.params).await,
            Method::DocumentSymbols => self.handle_document_symbols(request.params).await,
            Method::References => self.handle_references(request.params).await,
            Method::BatchReferences => self.handle_batch_references(request.params).await,
            Method::Inspect => self.handle_inspect(request.params).await,
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
        let params: HoverParams =
            serde_json::from_value(params).context("Invalid hover parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let file_str = params.file.to_string_lossy().to_string();
        client.open_document(&file_str).await?;

        let hover = Self::hover_with_warmup(&client, &file_str, params.line, params.column).await?;

        let result = HoverResult { hover };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a definition request.
    async fn handle_definition(&self, params: Value) -> Result<Value> {
        let params: DefinitionParams =
            serde_json::from_value(params).context("Invalid definition parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let file_str = params.file.to_string_lossy().to_string();
        client.open_document(&file_str).await?;
        let locations = client.goto_definition(&file_str, params.line, params.column).await?;

        let location = locations.into_iter().next();
        let result = DefinitionResult { location };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a workspace symbols request.
    async fn handle_workspace_symbols(&self, params: Value) -> Result<Value> {
        let params: WorkspaceSymbolsParams =
            serde_json::from_value(params).context("Invalid workspace symbols parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let mut symbols = Self::workspace_symbols_with_warmup(&client, &params.query).await?;

        // Filter by exact name if specified (avoids serializing thousands of fuzzy matches)
        if let Some(ref exact_name) = params.exact_name {
            symbols.retain(|s| s.name == *exact_name);
        }

        // Apply limit if specified
        if let Some(limit) = params.limit {
            symbols.truncate(limit);
        }

        let result = WorkspaceSymbolsResult { symbols };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a document symbols request.
    async fn handle_document_symbols(&self, params: Value) -> Result<Value> {
        let params: DocumentSymbolsParams =
            serde_json::from_value(params).context("Invalid document symbols parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let file_str = params.file.to_string_lossy().to_string();
        client.open_document(&file_str).await?;
        let symbols = client.document_symbols(&file_str).await?;

        let result = DocumentSymbolsResult { symbols };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a references request.
    async fn handle_references(&self, params: Value) -> Result<Value> {
        let params: ReferencesParams =
            serde_json::from_value(params).context("Invalid references parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let file_str = params.file.to_string_lossy().to_string();
        client.open_document(&file_str).await?;
        let locations = client
            .find_references(&file_str, params.line, params.column, params.include_declaration)
            .await?;

        let result = ReferencesResult { locations };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a batch references request (multiple queries, one connection).
    async fn handle_batch_references(&self, params: Value) -> Result<Value> {
        let params: BatchReferencesParams =
            serde_json::from_value(params).context("Invalid batch references parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let mut entries = Vec::with_capacity(params.queries.len());
        for q in &params.queries {
            let file_str = q.file.to_string_lossy().to_string();
            client.open_document(&file_str).await?;
            let locations = client
                .find_references(&file_str, q.line, q.column, params.include_declaration)
                .await?;
            entries.push(BatchReferencesEntry { label: q.label.clone(), locations });
        }

        let result = BatchReferencesResult { entries };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle an inspect request (hover, and optionally references).
    ///
    /// Requests are sequential because the LSP client communicates through a
    /// single stdin/stdout pipe — concurrent requests race on response routing.
    async fn handle_inspect(&self, params: Value) -> Result<Value> {
        let params: InspectParams =
            serde_json::from_value(params).context("Invalid inspect parameters")?;

        let client = { self.lsp_pool.lock().await.get_or_create(params.workspace).await? };

        let file_str = params.file.to_string_lossy().to_string();
        client.open_document(&file_str).await?;

        let hover = Self::hover_with_warmup(&client, &file_str, params.line, params.column).await?;

        let references = if params.include_references {
            client.find_references(&file_str, params.line, params.column, true).await?
        } else {
            Vec::new()
        };

        let result = InspectResult { hover, references };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle a diagnostics request.
    #[allow(clippy::unused_async)] // Matches async handler interface
    async fn handle_diagnostics(&self, _params: Value) -> Result<Value> {
        // Diagnostics are not yet implemented in the LSP client
        // Return empty diagnostics for now
        let result = DiagnosticsResult { diagnostics: vec![] };
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

    /// Hover with retry on cold start.
    ///
    /// The ty LSP server may return null hover when a document was recently
    /// opened and analysis hasn't completed. This is common on cold start
    /// (first daemon request) but can also happen when multiple handlers
    /// race to query a freshly-opened file. Retry a few times with
    /// increasing delays before giving up.
    async fn hover_with_warmup(
        client: &TyLspClient,
        file: &str,
        line: u32,
        column: u32,
    ) -> Result<Option<Hover>> {
        let hover = client.hover(file, line, column).await?;
        if hover.is_some() {
            return Ok(hover);
        }

        for delay_ms in [100, 200, 400] {
            tracing::debug!("hover returned null, retrying in {delay_ms}ms...");
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let hover = client.hover(file, line, column).await?;
            if hover.is_some() {
                return Ok(hover);
            }
        }

        tracing::debug!("hover still null after retries");
        Ok(None)
    }

    /// Workspace symbols with retry on cold start.
    ///
    /// On cold start the ty LSP server may not have finished indexing the
    /// workspace yet, returning zero symbols. Retry with back-off so callers
    /// (inspect, find, references) get results once indexing completes.
    async fn workspace_symbols_with_warmup(
        client: &TyLspClient,
        query: &str,
    ) -> Result<Vec<crate::lsp::protocol::SymbolInformation>> {
        let symbols = client.workspace_symbols(query).await?;
        if !symbols.is_empty() {
            return Ok(symbols);
        }

        for delay_ms in [100, 200, 400] {
            tracing::debug!("workspace symbols empty, retrying in {delay_ms}ms...");
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let symbols = client.workspace_symbols(query).await?;
            if !symbols.is_empty() {
                return Ok(symbols);
            }
        }

        tracing::debug!("workspace symbols still empty after retries");
        Ok(Vec::new())
    }

    /// Handle a shutdown request.
    #[allow(clippy::unused_async)] // Matches async handler interface
    async fn handle_shutdown(&self, _params: Value) -> Result<Value> {
        tracing::info!("Shutdown requested");

        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        let result = ShutdownResult { message: "Daemon shutting down".to_string() };
        Ok(serde_json::to_value(result)?)
    }

    /// Idle timeout task that shuts down the daemon after inactivity.
    async fn idle_timeout_task(&self) {
        let idle_timeout = Duration::from_secs(300); // 5 minutes
        let check_interval = Duration::from_secs(60); // 1 minute

        loop {
            tokio::time::sleep(check_interval).await;

            // Clean up idle LSP clients
            let pool = self.lsp_pool.lock().await;
            let removed = pool.cleanup_idle(idle_timeout);
            if removed > 0 {
                tracing::info!("Removed {removed} idle LSP clients");
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
    #[allow(clippy::unused_async)] // Called from async context
    async fn cleanup(&self) -> Result<()> {
        tracing::info!("Cleaning up daemon resources");

        // Remove socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).context("Failed to remove socket file")?;
        }

        Ok(())
    }
}

/// Send a framed error response to the client.
async fn send_error_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    error: DaemonError,
) -> Result<()> {
    let error_response = DaemonResponse::error(0, error);
    let response_json = serde_json::to_string(&error_response)?;
    let framed = format!("Content-Length: {}\r\n\r\n{response_json}", response_json.len());
    writer.write_all(framed.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
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
        let value = server.handle_ping(params).await.expect("ping should succeed");

        assert_eq!(value["status"], "running");
        assert_eq!(value["active_workspaces"], 0);
        assert_eq!(value["cache_size"], 0);
        // Uptime should be a small number since the server was just created
        assert!(value["uptime"].as_u64().unwrap() < 5);
    }
}
