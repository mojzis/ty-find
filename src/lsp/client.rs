use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::lsp::{protocol::*, server::TyLspServer};

pub struct TyLspClient {
    server: Arc<Mutex<TyLspServer>>,
    request_id: AtomicU64,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<LSPResponse>>>>,
}

impl TyLspClient {
    pub async fn new(workspace_root: &str) -> Result<Self> {
        let server = TyLspServer::start(workspace_root).await?;
        let client = Self {
            server: Arc::new(Mutex::new(server)),
            request_id: AtomicU64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        };

        client.initialize(workspace_root).await?;
        Ok(client)
    }

    async fn initialize(&self, workspace_root: &str) -> Result<()> {
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootPath": workspace_root,
            "rootUri": format!("file://{}", workspace_root),
            "capabilities": {
                "textDocument": {
                    "definition": {
                        "dynamicRegistration": false,
                        "linkSupport": true
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

        self.send_notification("initialized", serde_json::json!({}))
            .await?;

        Ok(())
    }

    pub async fn goto_definition(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>> {
        let uri = format!("file://{}", std::fs::canonicalize(file_path)?.display());

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self
            .send_request("textDocument/definition", serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            let locations: Vec<Location> = match result {
                Value::Array(arr) => serde_json::from_value(Value::Array(arr))?,
                Value::Object(_) => vec![serde_json::from_value(result)?],
                _ => vec![],
            };
            Ok(locations)
        } else {
            Ok(vec![])
        }
    }

    pub async fn find_references(
        &self,
        file_path: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let uri = format!("file://{}", std::fs::canonicalize(file_path)?.display());

        let params = ReferenceParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            context: ReferenceContext {
                include_declaration,
            },
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self
            .send_request("textDocument/references", serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            let locations: Vec<Location> = match result {
                Value::Array(arr) => serde_json::from_value(Value::Array(arr))?,
                Value::Null => vec![],
                _ => vec![],
            };
            Ok(locations)
        } else {
            Ok(vec![])
        }
    }

    pub async fn hover(&self, file_path: &str, line: u32, character: u32) -> Result<Option<Hover>> {
        let uri = format!("file://{}", std::fs::canonicalize(file_path)?.display());

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_token: None,
        };

        let response = self
            .send_request("textDocument/hover", serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            if result.is_null() {
                Ok(None)
            } else {
                let hover: Hover = serde_json::from_value(result)?;
                Ok(Some(hover))
            }
        } else {
            Ok(None)
        }
    }

    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolInformation>> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self
            .send_request("workspace/symbol", serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            let symbols: Vec<SymbolInformation> = match result {
                Value::Array(arr) => serde_json::from_value(Value::Array(arr))?,
                _ => vec![],
            };
            Ok(symbols)
        } else {
            Ok(vec![])
        }
    }

    pub async fn document_symbols(&self, file_path: &str) -> Result<Vec<DocumentSymbol>> {
        let uri = format!("file://{}", std::fs::canonicalize(file_path)?.display());

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self
            .send_request("textDocument/documentSymbol", serde_json::to_value(params)?)
            .await?;

        if let Some(result) = response.result {
            let symbols: Vec<DocumentSymbol> = match result {
                Value::Array(arr) => serde_json::from_value(Value::Array(arr))?,
                _ => vec![],
            };
            Ok(symbols)
        } else {
            Ok(vec![])
        }
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<LSPResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.insert(id, tx);
        }

        let request = LSPRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(id.into()),
            method: method.to_string(),
            params,
        };

        self.send_message(&request).await?;

        let response = rx.await?;
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
        let content = serde_json::to_string(message)?;
        self.send_raw_message(&content).await
    }

    #[allow(clippy::await_holding_lock)]
    async fn send_raw_message(&self, content: &str) -> Result<()> {
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);
        let mut server = self.server.lock().unwrap();
        let stdin = server.stdin();
        stdin.write_all(message.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn start_response_handler(&self) -> Result<()> {
        let server = Arc::clone(&self.server);
        let pending_requests = Arc::clone(&self.pending_requests);

        tokio::spawn(async move {
            let mut stdout = {
                let mut server_guard = server.lock().unwrap();
                server_guard.stdout()
            };

            let mut buffer = String::new();
            let mut content_length: Option<usize> = None;

            loop {
                buffer.clear();
                match stdout.read_line(&mut buffer).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if buffer.starts_with("Content-Length:") {
                            if let Some(len_str) =
                                buffer.strip_prefix("Content-Length:").map(|s| s.trim())
                            {
                                content_length = len_str.parse().ok();
                            }
                        } else if buffer.trim().is_empty() && content_length.is_some() {
                            let len = content_length.take().unwrap();
                            let mut content = vec![0; len];
                            if stdout.read_exact(&mut content).await.is_ok() {
                                if let Ok(response_str) = String::from_utf8(content) {
                                    if let Ok(response) =
                                        serde_json::from_str::<LSPResponse>(&response_str)
                                    {
                                        if let Value::Number(id_num) = &response.id {
                                            if let Some(id) = id_num.as_u64() {
                                                let mut pending = pending_requests.lock().unwrap();
                                                if let Some(sender) = pending.remove(&id) {
                                                    let _ = sender.send(response);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(())
    }
}
