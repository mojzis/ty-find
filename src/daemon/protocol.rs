//! JSON-RPC 2.0 protocol types for daemon communication.
//!
//! This module defines the communication protocol between the ty-find CLI
//! and the persistent daemon server. The protocol uses JSON-RPC 2.0 over
//! Unix domain sockets.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::path::PathBuf;

// Re-export LSP types that are used in responses
pub use crate::lsp::protocol::{DocumentSymbol, Hover, Location, Range, SymbolInformation};

/// JSON-RPC 2.0 request from CLI to daemon.
///
/// # Example
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "id": 1,
///   "method": "hover",
///   "params": {
///     "workspace": "/path/to/workspace",
///     "file": "/path/to/file.py",
///     "line": 10,
///     "column": 5
///   }
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonRequest {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Unique request identifier
    pub id: u64,

    /// Method name to invoke
    pub method: Method,

    /// Method-specific parameters
    pub params: Value,
}

impl DaemonRequest {
    /// Create a new daemon request with auto-generated ID.
    pub fn new(method: Method, params: Value) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            jsonrpc: "2.0".to_string(),
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            method,
            params,
        }
    }

    /// Create a request with a specific ID.
    pub fn with_id(id: u64, method: Method, params: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, method, params }
    }
}

/// JSON-RPC 2.0 response from daemon to CLI.
///
/// Either `result` or `error` will be present, but not both.
///
/// # Success Example
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "id": 1,
///   "result": {
///     "symbol": "foo",
///     "type": "str"
///   }
/// }
/// ```
///
/// # Error Example
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "id": 1,
///   "error": {
///     "code": -32000,
///     "message": "File not found",
///     "data": {"file": "/path/to/file.py"}
///   }
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonResponse {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,

    /// Request ID this response corresponds to
    pub id: u64,

    /// Successful result (mutually exclusive with error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Error result (mutually exclusive with result)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DaemonError>,
}

impl DaemonResponse {
    /// Create a successful response.
    pub fn success(id: u64, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    /// Create an error response.
    pub fn error(id: u64, error: DaemonError) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(error) }
    }

    /// Check if this response represents an error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Check if this response represents a success.
    pub fn is_success(&self) -> bool {
        self.result.is_some()
    }
}

/// JSON-RPC 2.0 error object.
///
/// Error codes follow JSON-RPC conventions with custom application errors
/// starting at -32000.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonError {
    /// Error code
    pub code: i32,

    /// Human-readable error message
    pub message: String,

    /// Additional error data (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl DaemonError {
    /// Create a new daemon error.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    /// Create an error with additional data.
    pub fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self { code, message: message.into(), data: Some(data) }
    }

    // Standard JSON-RPC errors

    /// Parse error (-32700): Invalid JSON
    pub fn parse_error() -> Self {
        Self::new(-32700, "Parse error")
    }

    /// Invalid request (-32600): Invalid request object
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::new(-32600, msg)
    }

    /// Method not found (-32601): Unknown method
    pub fn method_not_found(method: impl Into<String>) -> Self {
        let method = method.into();
        Self::new(-32601, format!("Method not found: {method}"))
    }

    /// Invalid params (-32602): Invalid method parameters
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(-32602, msg)
    }

    /// Internal error (-32603): Internal JSON-RPC error
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::new(-32603, msg)
    }

    // Application-specific errors (starting at -32000)

    /// File not found error (-32000)
    pub fn file_not_found(file: impl Into<String>) -> Self {
        let file = file.into();
        Self::with_data(-32000, "File not found", serde_json::json!({"file": file}))
    }

    /// Workspace not found error (-32001)
    pub fn workspace_not_found(workspace: impl Into<String>) -> Self {
        let workspace = workspace.into();
        Self::with_data(-32001, "Workspace not found", serde_json::json!({"workspace": workspace}))
    }

    /// LSP server error (-32002)
    pub fn lsp_error(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self::new(-32002, format!("LSP error: {msg}"))
    }

    /// Timeout error (-32003)
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::with_data(
            -32003,
            "Operation timed out",
            serde_json::json!({"operation": operation.into()}),
        )
    }

    /// Symbol not found error (-32004)
    pub fn symbol_not_found(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        Self::with_data(-32004, "Symbol not found", serde_json::json!({"symbol": symbol}))
    }
}

/// Supported daemon methods.
///
/// Each method corresponds to a specific LSP operation or daemon command.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Get hover information (type, docs) at a position
    Hover,

    /// Go to definition at a position
    Definition,

    /// Search for symbols across the workspace
    WorkspaceSymbols,

    /// Get document outline (all symbols in a file)
    DocumentSymbols,

    /// Find all references to a symbol at a position
    References,

    /// Inspect a symbol: hover + references in one call (parallelized server-side)
    Inspect,

    /// Get diagnostics (type errors, warnings) for a file
    Diagnostics,

    /// Health check - verify daemon is responsive
    Ping,

    /// Gracefully shutdown the daemon
    Shutdown,
}

impl Method {
    /// Get the method name as a string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Definition => "definition",
            Self::WorkspaceSymbols => "workspace_symbols",
            Self::DocumentSymbols => "document_symbols",
            Self::References => "references",
            Self::Inspect => "inspect",
            Self::Diagnostics => "diagnostics",
            Self::Ping => "ping",
            Self::Shutdown => "shutdown",
        }
    }
}

// ============================================================================
// Request parameter types for each method
// ============================================================================

/// Parameters for hover request.
///
/// Returns type information and documentation at a specific position.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HoverParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,

    /// Line number (0-based)
    pub line: u32,

    /// Column number (0-based)
    pub column: u32,
}

/// Parameters for definition request.
///
/// Returns the location where a symbol is defined.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DefinitionParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,

    /// Line number (0-based)
    pub line: u32,

    /// Column number (0-based)
    pub column: u32,
}

/// Parameters for workspace symbols request.
///
/// Searches for symbols matching a query across the entire workspace.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceSymbolsParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// Search query (can be fuzzy)
    pub query: String,

    /// Maximum number of results to return (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Parameters for document symbols request.
///
/// Returns an outline of all symbols in a file.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentSymbolsParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,
}

/// Parameters for references request.
///
/// Returns all locations where a symbol is referenced.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReferencesParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,

    /// Line number (0-based)
    pub line: u32,

    /// Column number (0-based)
    pub column: u32,

    /// Whether to include the declaration in results
    pub include_declaration: bool,
}

/// Parameters for inspect request.
///
/// Runs hover and optionally references on the daemon side.
/// When references are included, hover and references run in parallel.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InspectParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,

    /// Line number (0-based)
    pub line: u32,

    /// Column number (0-based)
    pub column: u32,

    /// Whether to include references (can be slow on large codebases)
    #[serde(default)]
    pub include_references: bool,
}

/// Parameters for diagnostics request.
///
/// Returns type errors and warnings for a file.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiagnosticsParams {
    /// Workspace root directory
    pub workspace: PathBuf,

    /// File path (absolute or relative to workspace)
    pub file: PathBuf,
}

/// Parameters for ping request.
///
/// Health check with no parameters.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingParams {}

/// Parameters for shutdown request.
///
/// Graceful shutdown with no parameters.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShutdownParams {}

// ============================================================================
// Response result types for each method
// ============================================================================

/// Result of a hover request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HoverResult {
    /// Hover information (if found)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<Hover>,
}

/// Result of a definition request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DefinitionResult {
    /// Definition location (if found)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

/// Result of a workspace symbols request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceSymbolsResult {
    /// List of matching symbols
    pub symbols: Vec<SymbolInformation>,
}

/// Result of a document symbols request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentSymbolsResult {
    /// Hierarchical symbol tree
    pub symbols: Vec<DocumentSymbol>,
}

/// Result of a references request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReferencesResult {
    /// List of reference locations
    pub locations: Vec<Location>,
}

/// Result of an inspect request (hover + references combined).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InspectResult {
    /// Hover information (if found)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<Hover>,

    /// Reference locations
    pub references: Vec<Location>,
}

/// A single diagnostic message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Diagnostic {
    /// Range where the diagnostic applies
    pub range: Range,

    /// Severity level
    pub severity: DiagnosticSeverity,

    /// Diagnostic code (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Source of the diagnostic (e.g., "ty", "pyright")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Diagnostic message
    pub message: String,

    /// Related information (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_information: Option<Vec<DiagnosticRelatedInformation>>,
}

/// Severity level of a diagnostic.
#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// Related information for a diagnostic.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiagnosticRelatedInformation {
    /// Location of related information
    pub location: Location,

    /// Message describing the relation
    pub message: String,
}

/// Result of a diagnostics request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiagnosticsResult {
    /// List of diagnostics for the file
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of a ping request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PingResult {
    /// Daemon status message
    pub status: String,

    /// Daemon uptime in seconds
    pub uptime: u64,

    /// Number of active workspaces
    pub active_workspaces: usize,

    /// Number of cached responses
    pub cache_size: usize,
}

/// Result of a shutdown request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShutdownResult {
    /// Shutdown confirmation message
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_daemon_request_serialization() {
        let request = DaemonRequest::with_id(
            1,
            Method::Hover,
            json!({
                "workspace": "/path/to/workspace",
                "file": "/path/to/file.py",
                "line": 10,
                "column": 5
            }),
        );

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"hover\""));
    }

    #[test]
    fn test_daemon_response_success() {
        let response = DaemonResponse::success(1, json!({"status": "ok"}));

        assert!(response.is_success());
        assert!(!response.is_error());
        assert_eq!(response.id, 1);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_daemon_response_error() {
        let error = DaemonError::file_not_found("/path/to/file.py");
        let response = DaemonResponse::error(1, error);

        assert!(response.is_error());
        assert!(!response.is_success());

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_method_serialization() {
        assert_eq!(serde_json::to_string(&Method::Hover).unwrap(), "\"hover\"");
        assert_eq!(serde_json::to_string(&Method::Definition).unwrap(), "\"definition\"");
        assert_eq!(
            serde_json::to_string(&Method::WorkspaceSymbols).unwrap(),
            "\"workspace_symbols\""
        );
    }

    #[test]
    fn test_hover_params() {
        let params = HoverParams {
            workspace: PathBuf::from("/workspace"),
            file: PathBuf::from("file.py"),
            line: 10,
            column: 5,
        };

        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["line"], 10);
        assert_eq!(json["column"], 5);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(DaemonError::parse_error().code, -32700);
        assert_eq!(DaemonError::invalid_request("test").code, -32600);
        assert_eq!(DaemonError::method_not_found("test").code, -32601);
        assert_eq!(DaemonError::file_not_found("test").code, -32000);
        assert_eq!(DaemonError::workspace_not_found("test").code, -32001);
    }

    #[test]
    fn test_diagnostic_severity() {
        assert_eq!(DiagnosticSeverity::Error as u8, 1);
        assert_eq!(DiagnosticSeverity::Warning as u8, 2);
        assert_eq!(DiagnosticSeverity::Information as u8, 3);
        assert_eq!(DiagnosticSeverity::Hint as u8, 4);
    }
}
