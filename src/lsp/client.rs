use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

use crate::lsp::protocol::{
    DocumentSymbol, DocumentSymbolParams, GotoDefinitionParams, Hover, HoverParams, LSPRequest,
    LSPResponse, Location, Position, ReferenceContext, ReferenceParams, SymbolInformation,
    TextDocumentIdentifier, TextDocumentPositionParams, WorkspaceSymbolParams,
};
use crate::lsp::server::TyLspServer;

pub struct TyLspClient {
    /// Kept alive so the child process is killed when the client is dropped.
    _server: TyLspServer,
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    request_id: AtomicU64,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<LSPResponse>>>>,
    /// URIs of documents already sent via `textDocument/didOpen`.
    /// Duplicate opens violate LSP protocol and can cause the server to
    /// re-analyze the file, returning null hover during the re-analysis window.
    opened_documents: Mutex<HashSet<String>>,
}

/// Build a `file://` URI from a file path, canonicalizing it first.
async fn file_uri(file_path: &str) -> Result<String> {
    let canonical = tokio::fs::canonicalize(file_path)
        .await
        .with_context(|| format!("Failed to resolve path: {file_path}"))?;
    Ok(format!("file://{}", canonical.display()))
}

/// Parse an LSP response that returns an array of items.
fn parse_response_array<T: DeserializeOwned>(response: LSPResponse) -> Result<Vec<T>> {
    match response.result {
        Some(Value::Array(arr)) => {
            serde_json::from_value(Value::Array(arr)).context("Failed to parse LSP response array")
        }
        _ => Ok(vec![]),
    }
}

impl TyLspClient {
    pub async fn new(workspace_root: &str) -> Result<Self> {
        let mut server =
            TyLspServer::start(workspace_root).await.context("Failed to start ty LSP server")?;

        let stdin = server.take_stdin();
        let stdout = server.take_stdout();

        let client = Self {
            _server: server,
            stdin: tokio::sync::Mutex::new(stdin),
            request_id: AtomicU64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            opened_documents: Mutex::new(HashSet::new()),
        };

        // Must start reading responses before sending initialize,
        // otherwise the initialize response is never consumed and we deadlock.
        client.start_response_handler(stdout);
        tracing::debug!("Sending LSP initialize request...");
        client.initialize(workspace_root).await.context("Failed to initialize LSP session")?;
        tracing::debug!("LSP client initialized successfully");
        Ok(client)
    }

    async fn initialize(&self, workspace_root: &str) -> Result<()> {
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootPath": workspace_root,
            "rootUri": format!("file://{workspace_root}"),
            "capabilities": {
                "textDocument": {
                    "definition": {
                        "dynamicRegistration": false,
                        "linkSupport": false
                    },
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "references": {
                        "dynamicRegistration": false
                    },
                    "documentSymbol": {
                        "dynamicRegistration": false,
                        "hierarchicalDocumentSymbolSupport": true
                    }
                },
                "workspace": {
                    "symbol": {
                        "dynamicRegistration": false
                    }
                }
            }
        });

        let _response = self.send_request("initialize", init_params).await?;

        self.send_notification("initialized", serde_json::json!({})).await?;

        Ok(())
    }

    /// Open a document and return whether it was newly opened.
    ///
    /// Returns `true` if this was the first `didOpen` for this URI.
    /// Returns `false` if the document was already open (no notification sent).
    ///
    /// LSP protocol requires exactly one `didOpen` per document. Sending it
    /// again causes the server to re-analyze from scratch, which can make
    /// hover/references return null during the re-analysis window.
    pub async fn open_document(&self, file_path: &str) -> Result<bool> {
        let uri = file_uri(file_path).await?;

        {
            let mut opened = self.opened_documents.lock().expect("opened_documents mutex poisoned");
            if !opened.insert(uri.clone()) {
                tracing::debug!("open_document: already open, skipping didOpen for {uri}");
                return Ok(false);
            }
        }

        let text = tokio::fs::read_to_string(file_path)
            .await
            .with_context(|| format!("Failed to read file: {file_path}"))?;

        self.send_notification(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "python",
                    "version": 1,
                    "text": text
                }
            }),
        )
        .await?;

        Ok(true)
    }

    pub async fn goto_definition(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>> {
        let uri = file_uri(file_path).await?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_token: None,
            partial_result_token: None,
        };

        let response =
            self.send_request("textDocument/definition", serde_json::to_value(params)?).await?;

        // Definition can return a single Location or an array of Locations
        match response.result {
            Some(Value::Array(arr)) => serde_json::from_value(Value::Array(arr))
                .context("Failed to parse definition locations"),
            Some(value @ Value::Object(_)) => {
                let loc: Location =
                    serde_json::from_value(value).context("Failed to parse definition location")?;
                Ok(vec![loc])
            }
            _ => Ok(vec![]),
        }
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let uri = file_uri(file_path).await?;

        let params = ReferenceParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            context: ReferenceContext { include_declaration },
            work_done_token: None,
            partial_result_token: None,
        };

        let response =
            self.send_request("textDocument/references", serde_json::to_value(params)?).await?;

        parse_response_array(response)
    }

    pub async fn hover(&self, file_path: &str, line: u32, character: u32) -> Result<Option<Hover>> {
        let uri = file_uri(file_path).await?;

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_token: None,
        };

        let response =
            self.send_request("textDocument/hover", serde_json::to_value(params)?).await?;

        match response.result {
            Some(value) if !value.is_null() => {
                let hover: Hover =
                    serde_json::from_value(value).context("Failed to parse hover response")?;
                Ok(Some(hover))
            }
            _ => Ok(None),
        }
    }

    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolInformation>> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self.send_request("workspace/symbol", serde_json::to_value(params)?).await?;

        parse_response_array(response)
    }

    pub async fn document_symbols(&self, file_path: &str) -> Result<Vec<DocumentSymbol>> {
        let uri = file_uri(file_path).await?;

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_token: None,
            partial_result_token: None,
        };

        let response =
            self.send_request("textDocument/documentSymbol", serde_json::to_value(params)?).await?;

        parse_response_array(response)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<LSPResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        {
            let mut pending =
                self.pending_requests.lock().expect("pending_requests mutex poisoned");
            pending.insert(id, tx);
        }

        let request = LSPRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(id.into()),
            method: method.to_string(),
            params,
        };

        tracing::debug!("Sending LSP request: {method} (id: {id})");
        self.send_message(&request).await?;

        let response = rx.await.context("LSP response channel closed unexpectedly")?;

        if let Some(ref error) = response.error {
            tracing::debug!("LSP error response for {method} (id: {id}): {error:?}");
        } else {
            tracing::debug!("LSP response received for {method} (id: {id})");
        }

        Ok(response)
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_raw_message(&notification.to_string()).await
    }

    async fn send_message<T: serde::Serialize>(&self, message: &T) -> Result<()> {
        let content = serde_json::to_string(message).context("Failed to serialize LSP message")?;
        self.send_raw_message(&content).await
    }

    async fn send_raw_message(&self, content: &str) -> Result<()> {
        let message = format!("Content-Length: {}\r\n\r\n{content}", content.len());
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(message.as_bytes()).await.context("Failed to write to LSP stdin")?;
        stdin.flush().await.context("Failed to flush LSP stdin")?;
        Ok(())
    }

    fn start_response_handler(&self, stdout: BufReader<tokio::process::ChildStdout>) {
        let pending_requests = Arc::clone(&self.pending_requests);

        // JoinHandle intentionally not stored — the task exits naturally when
        // the server's stdout closes (EOF), which happens when TyLspServer is
        // dropped and the child process is killed.
        tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buffer = String::new();
            let mut content_length: Option<usize> = None;

            loop {
                buffer.clear();
                match stdout.read_line(&mut buffer).await {
                    Ok(0) => {
                        tracing::debug!("LSP server stdout closed (EOF)");
                        break;
                    }
                    Ok(_) => {
                        if buffer.starts_with("Content-Length:") {
                            if let Some(len_str) =
                                buffer.strip_prefix("Content-Length:").map(str::trim)
                            {
                                content_length = len_str.parse().ok();
                            }
                        } else if buffer.trim().is_empty() {
                            if let Some(len) = content_length.take() {
                                let mut content = vec![0; len];
                                if stdout.read_exact(&mut content).await.is_ok() {
                                    if let Ok(response_str) = String::from_utf8(content) {
                                        if let Ok(response) =
                                            serde_json::from_str::<LSPResponse>(&response_str)
                                        {
                                            if let Value::Number(id_num) = &response.id {
                                                if let Some(id) = id_num.as_u64() {
                                                    let mut pending = pending_requests
                                                        .lock()
                                                        .expect("pending_requests mutex poisoned");
                                                    if let Some(sender) = pending.remove(&id) {
                                                        let _ = sender.send(response);
                                                    }
                                                }
                                            }
                                        } else {
                                            tracing::debug!(
                                                "Failed to parse LSP response: {}",
                                                response_str.chars().take(200).collect::<String>()
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("LSP server stdout read error: {e}");
                        break;
                    }
                }
            }
        });
    }
}
