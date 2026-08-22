//! Daemon-startup concurrency for the `tyf mcp` frontend.
//!
//! This lives in its own test target on purpose: it stops the shared daemon
//! and then counts daemons, which only means anything if no other test is
//! racing it. Cargo runs test binaries one at a time, and this one holds a
//! single test, so nothing else touches the daemon while it runs.

#[path = "common.rs"]
mod common;
#[path = "mcp_session.rs"]
mod mcp_session;

use mcp_session::{is_error, running_daemon_count, stop_daemon, workspace_root, McpSession};
use serde_json::json;

/// A harness may pipeline tool calls. Daemon startup has to be serialized
/// within the process, or each concurrent first call spawns its own daemon
/// (and its own `ty lsp`), leaving orphans behind.
#[test]
fn test_concurrent_tool_calls_start_exactly_one_daemon() {
    common::require_ty();
    stop_daemon();

    let mut session = McpSession::start(&workspace_root());
    let calls = [
        json!({ "name": "show", "arguments": { "symbols": ["hello_world"] } }),
        json!({ "name": "find", "arguments": { "symbols": ["calculate_sum"] } }),
        json!({ "name": "list", "arguments": { "file": "example.py" } }),
        json!({ "name": "refs", "arguments": { "queries": ["Calculator"] } }),
    ];

    // Write every frame before reading any response, so the server has all
    // four in flight at once.
    let ids: Vec<i64> =
        calls.iter().map(|params| session.send_request("tools/call", params)).collect();
    for id in ids {
        let response = session.read_response(id);
        assert!(response.get("error").is_none(), "concurrent call failed: {response}");
        assert!(
            !is_error(&response["result"]),
            "concurrent call returned a tool error: {response}"
        );
    }

    assert_eq!(
        running_daemon_count(),
        1,
        "concurrent first calls must share one daemon, not spawn one each"
    );
}
