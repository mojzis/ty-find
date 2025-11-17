//! Daemon module for persistent LSP server connections.
//!
//! This module provides daemon functionality to keep LSP connections alive
//! between CLI invocations, enabling fast response times (<100ms) for
//! subsequent requests.

pub mod client;
pub mod pool;
pub mod protocol;
pub mod server;

// Re-export main types for convenience
pub use client::{DaemonClient, ensure_daemon_running, get_socket_path};
pub use pool::LspClientPool;
pub use server::DaemonServer;
pub use protocol::{
    DaemonError, DaemonRequest, DaemonResponse, Method,
    // Request types
    HoverParams, DefinitionParams, WorkspaceSymbolsParams,
    DocumentSymbolsParams, DiagnosticsParams, PingParams,
    // Response types
    HoverResult, DefinitionResult, WorkspaceSymbolsResult,
    DocumentSymbolsResult, DiagnosticsResult, PingResult,
};
