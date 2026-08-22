//! Shared harness for driving a real `tyf mcp` process over stdio.
//!
//! Included by every MCP integration target, so some items are unused in any
//! single one of them.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The MCP revision this server is built against.
pub const PROTOCOL_VERSION: &str = "2026-07-28";

/// Repo root — the workspace all fixtures live in.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A live `tyf mcp` process being driven over stdio.
pub struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    /// Responses read while waiting for a different id. The server answers
    /// pipelined requests in whatever order they finish, so a response that
    /// arrives early has to be kept, not dropped.
    pending: HashMap<i64, Value>,
}

impl McpSession {
    /// Spawn `tyf mcp --workspace <root>` and complete the initialize handshake.
    pub fn start(workspace: &Path) -> Self {
        Self::start_in(workspace, Some(workspace))
    }

    /// Spawn `tyf mcp`, optionally passing `--workspace`, with the process cwd
    /// set to `cwd`. Passing `workspace: None` exercises cwd-based resolution.
    pub fn start_in(cwd: &Path, workspace: Option<&Path>) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tyf"));
        cmd.arg("mcp");
        if let Some(ws) = workspace {
            cmd.arg("--workspace").arg(ws);
        }
        let mut child = cmd
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn `tyf mcp`");

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
        let mut session = Self { child, stdin, stdout, next_id: 1, pending: HashMap::new() };
        session.initialize();
        session
    }

    pub fn initialize(&mut self) {
        let result = self.request(
            "initialize",
            &json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "tyf-integration-test", "version": "0" },
            }),
        );
        assert_eq!(
            result["protocolVersion"], PROTOCOL_VERSION,
            "server must negotiate {PROTOCOL_VERSION}, got: {result}"
        );
        self.notify("notifications/initialized", &json!({}));
    }

    pub fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("failed to write to `tyf mcp` stdin");
        self.stdin.flush().expect("failed to flush `tyf mcp` stdin");
    }

    pub fn notify(&mut self, method: &str, params: &Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Send a request and return its `result`, panicking on a JSON-RPC error.
    pub fn request(&mut self, method: &str, params: &Value) -> Value {
        let response = self.raw_request(method, params);
        assert!(response.get("error").is_none(), "{method} returned a JSON-RPC error: {response}");
        response["result"].clone()
    }

    /// Send a request and return the whole response envelope.
    pub fn raw_request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.send_request(method, params);
        self.read_response(id)
    }

    /// Write a request without waiting for its response, returning its id.
    pub fn send_request(&mut self, method: &str, params: &Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    /// Read until the response carrying `id` is available, buffering responses
    /// to other ids and skipping notifications.
    ///
    /// A pipelining caller must not lose the responses that land first: the
    /// server answers concurrent requests in whatever order they finish.
    pub fn read_response(&mut self, id: i64) -> Value {
        if let Some(message) = self.pending.remove(&id) {
            return message;
        }
        loop {
            let mut line = String::new();
            let read =
                self.stdout.read_line(&mut line).expect("failed to read from `tyf mcp` stdout");
            assert!(read > 0, "`tyf mcp` closed stdout while awaiting response id {id}");
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value = serde_json::from_str(trimmed)
                .unwrap_or_else(|e| panic!("`tyf mcp` wrote non-JSON to stdout: {e}\n{trimmed}"));
            match message.get("id").and_then(Value::as_i64) {
                Some(other) if other == id => return message,
                // A response to another pipelined request — keep it for later.
                Some(other) => {
                    self.pending.insert(other, message);
                }
                // A notification; nothing here asserts on those.
                None => {}
            }
        }
    }

    /// Call a tool and return the `CallToolResult`.
    pub fn call_tool(&mut self, name: &str, arguments: &Value) -> Value {
        self.request("tools/call", &json!({ "name": name, "arguments": arguments }))
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        // Closing stdin is the documented shutdown path; kill is the backstop.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Concatenate the text blocks of a `CallToolResult`.
pub fn tool_text(result: &Value) -> String {
    let content = result["content"].as_array().expect("content must be an array");
    content
        .iter()
        .filter(|block| block["type"] == "text")
        .map(|block| block["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("")
}

/// Whether the tool reported a tool-level error (`isError: true`).
pub fn is_error(result: &Value) -> bool {
    result["isError"].as_bool().unwrap_or(false)
}

/// Stop any running daemon so the next tool call starts one cold.
pub fn stop_daemon() {
    let _ = Command::new(env!("CARGO_BIN_EXE_tyf"))
        .args(["daemon", "stop"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// How many daemons built from this test binary's `tyf` are running.
pub fn running_daemon_count() -> usize {
    let output = Command::new("ps").args(["-eo", "args"]).output().expect("failed to run ps");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with(env!("CARGO_BIN_EXE_tyf")) && line.contains("daemon start"))
        .count()
}
