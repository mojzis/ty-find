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
#[allow(unused_imports)]
pub use client::{ensure_daemon_running, get_socket_path, spawn_daemon, DaemonClient};
#[allow(unused_imports)]
pub use pool::LspClientPool;
#[allow(unused_imports)]
pub use protocol::{
    DaemonError, DaemonRequest, DaemonResponse, DefinitionParams, DefinitionResult,
    DiagnosticsParams, DiagnosticsResult, DocumentSymbolsParams, DocumentSymbolsResult,
    HoverParams, HoverResult, Method, PingParams, PingResult, WorkspaceSymbolsParams,
    WorkspaceSymbolsResult,
};
#[allow(unused_imports)]
pub use server::DaemonServer;
