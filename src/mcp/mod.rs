//! The MCP server frontend.
//!
//! A thin bridge, parallel to the CLI: it speaks MCP over stdio and translates
//! each tool call into the same command handler the CLI calls, over the same
//! Unix-socket daemon connection. It performs no LSP work of its own.

pub mod server;

pub use server::serve_stdio;
