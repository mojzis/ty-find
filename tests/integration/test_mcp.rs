//! Integration tests for the `tyf mcp` frontend.
//!
//! Each test spawns the real `tyf mcp` binary, drives it with raw MCP
//! JSON-RPC over stdio, and asserts on the tool results it returns. The
//! daemon and `ty lsp` are real too — the bridge is only exercised
//! end-to-end.

#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The MCP revision this server is built against.
const PROTOCOL_VERSION: &str = "2026-07-28";

/// Ceiling on the serialized `tools/list` response, in characters.
///
/// Tool definitions are injected into an agent's context every session, so
/// schema bloat is a product regression, not a style nit. ~4000 chars is the
/// proxy for the ~1000-token budget. The unit test in `src/mcp/server.rs`
/// asserts the same ceiling without spawning a process; this one proves the
/// number holds for what actually goes over the wire.
const TOOL_LIST_CHAR_CEILING: usize = 4000;

/// Repo root — the workspace all fixtures live in.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A live `tyf mcp` process being driven over stdio.
struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpSession {
    /// Spawn `tyf mcp --workspace <root>` and complete the initialize handshake.
    fn start(workspace: &Path) -> Self {
        Self::start_in(workspace, Some(workspace))
    }

    /// Spawn `tyf mcp`, optionally passing `--workspace`, with the process cwd
    /// set to `cwd`. Passing `workspace: None` exercises cwd-based resolution.
    fn start_in(cwd: &Path, workspace: Option<&Path>) -> Self {
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
        let mut session = Self { child, stdin, stdout, next_id: 1 };
        session.initialize();
        session
    }

    fn initialize(&mut self) {
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

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("failed to write to `tyf mcp` stdin");
        self.stdin.flush().expect("failed to flush `tyf mcp` stdin");
    }

    fn notify(&mut self, method: &str, params: &Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Send a request and return its `result`, panicking on a JSON-RPC error.
    fn request(&mut self, method: &str, params: &Value) -> Value {
        let response = self.raw_request(method, params);
        assert!(response.get("error").is_none(), "{method} returned a JSON-RPC error: {response}");
        response["result"].clone()
    }

    /// Send a request and return the whole response envelope.
    fn raw_request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        self.read_response(id)
    }

    /// Read lines until the response carrying `id` arrives, skipping
    /// notifications and any other traffic the server emits.
    fn read_response(&mut self, id: i64) -> Value {
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
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
    }

    /// Call a tool and return the `CallToolResult`.
    fn call_tool(&mut self, name: &str, arguments: &Value) -> Value {
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
fn tool_text(result: &Value) -> String {
    let content = result["content"].as_array().expect("content must be an array");
    content
        .iter()
        .filter(|block| block["type"] == "text")
        .map(|block| block["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("")
}

/// Whether the tool reported a tool-level error (`isError: true`).
fn is_error(result: &Value) -> bool {
    result["isError"].as_bool().unwrap_or(false)
}

/// Run the CLI and return its stdout.
fn cli_stdout(args: &[&str]) -> String {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(workspace_root()).arg("--color").arg("never").args(args);
    let output = cmd.output().expect("failed to run tyf");
    assert!(
        output.status.success(),
        "tyf {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("tyf stdout must be UTF-8")
}

#[test]
fn test_tools_list_shape_and_budget() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session.request("tools/list", &json!({}));
    let tools = result["tools"].as_array().expect("tools must be an array");

    let mut names: Vec<&str> =
        tools.iter().map(|t| t["name"].as_str().expect("tool name")).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["find", "list", "members", "refs", "show"],
        "exactly the five CLI-mirroring tools must be exposed"
    );

    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        let description = tool["description"].as_str().unwrap_or_default();
        assert!(!description.is_empty(), "tool '{name}' needs a description");
        assert_eq!(
            tool["inputSchema"]["type"], "object",
            "tool '{name}' needs an object input schema"
        );
        assert!(
            tool.get("outputSchema").is_none(),
            "tool '{name}' must not declare an outputSchema"
        );
    }

    let serialized = serde_json::to_string(&result).expect("tools/list must serialize");
    assert!(
        serialized.chars().count() <= TOOL_LIST_CHAR_CEILING,
        "serialized tools/list is {} chars, over the {TOOL_LIST_CHAR_CEILING} ceiling. \
         Tool schemas are injected into every agent session — tighten the descriptions \
         rather than raising the ceiling.\n{serialized}",
        serialized.chars().count()
    );
}

#[test]
fn test_show_single_and_batched() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let single = session.call_tool("show", &json!({ "symbols": ["hello_world"] }));
    assert!(!is_error(&single), "show should succeed: {single}");
    let text = tool_text(&single);
    assert!(text.contains("hello_world"), "show output should name the symbol:\n{text}");
    assert!(text.contains("example.py:1:"), "show output should locate it:\n{text}");

    let batched =
        session.call_tool("show", &json!({ "symbols": ["hello_world", "calculate_sum"] }));
    assert!(!is_error(&batched), "batched show should succeed: {batched}");
    let text = tool_text(&batched);
    assert!(text.contains("hello_world"), "batched show should cover the first:\n{text}");
    assert!(text.contains("calculate_sum"), "batched show should cover the second:\n{text}");
}

#[test]
fn test_show_doc_flag_adds_docstring() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let without = tool_text(&session.call_tool("show", &json!({ "symbols": ["Animal"] })));
    assert!(
        !without.contains("Base class for animals"),
        "the docstring is opt-in, like the CLI's --doc:\n{without}"
    );

    let with =
        tool_text(&session.call_tool("show", &json!({ "symbols": ["Animal"], "doc": true })));
    assert!(
        with.contains("Base class for animals"),
        "`doc: true` should add the docstring:\n{with}"
    );
}

#[test]
fn test_find_fuzzy() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session.call_tool("find", &json!({ "symbols": ["hello_"], "fuzzy": true }));
    assert!(!is_error(&result), "fuzzy find should succeed: {result}");
    let text = tool_text(&result);
    assert!(
        text.contains("hello_world"),
        "fuzzy find on a prefix should match hello_world:\n{text}"
    );
}

#[test]
fn test_dotted_notation_resolves_the_right_class() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session.call_tool("show", &json!({ "symbols": ["Database.get_data"] }));
    assert!(!is_error(&result), "dotted show should succeed: {result}");
    let text = tool_text(&result);
    assert!(
        text.contains("dotted_fixture.py:10:") || text.contains("dotted_fixture.py:11:"),
        "Database.get_data must resolve to Database's method, not Cache's:\n{text}"
    );
}

/// 2+ dots, a leading dot and a trailing dot are usage errors on the CLI
/// (exit 2, message on stderr). Over MCP they become tool errors carrying the
/// same message text.
#[test]
fn test_dotted_notation_usage_errors() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    for bad in ["Outer.Inner.method", ".leading", "trailing."] {
        let result = session.call_tool("show", &json!({ "symbols": [bad] }));
        assert!(is_error(&result), "'{bad}' must be a tool error, got: {result}");
        let text = tool_text(&result);
        assert!(
            text.contains(bad) && text.contains("dotted notation supports one level only"),
            "'{bad}' must carry the CLI's usage message, got:\n{text}"
        );
    }
}

/// A well-formed query that matches nothing is a normal success result on the
/// CLI (exit 0) and must stay one over MCP.
#[test]
fn test_valid_query_matching_nothing_is_not_an_error() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result =
        session.call_tool("find", &json!({ "symbols": ["definitely_not_a_real_symbol_xyz"] }));
    assert!(!is_error(&result), "a valid query matching nothing must succeed, not error: {result}");
    assert!(!tool_text(&result).is_empty(), "the miss should still be reported in text");
}

#[test]
fn test_refs_position_mode() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    // `hello_world` is defined at example.py line 1, column 5 (1-indexed).
    let result = session.call_tool("refs", &json!({ "queries": ["example.py:1:5"] }));
    assert!(!is_error(&result), "refs position mode should succeed: {result}");
    let text = tool_text(&result);
    assert!(
        text.contains("example.py:1:5"),
        "refs should label the result with the queried position:\n{text}"
    );
    assert!(
        text.contains("example.py:18") || text.contains("example.py:1"),
        "refs should report the call site in main():\n{text}"
    );
}

#[test]
fn test_refs_symbol_mode() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session.call_tool("refs", &json!({ "queries": ["calculate_sum"] }));
    assert!(!is_error(&result), "refs symbol mode should succeed: {result}");
    assert!(
        tool_text(&result).contains("calculate_sum"),
        "refs should name the queried symbol:\n{}",
        tool_text(&result)
    );
}

#[test]
fn test_members() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session
        .call_tool("members", &json!({ "symbols": ["Animal"], "file": "members_example.py" }));
    assert!(!is_error(&result), "members should succeed: {result}");
    let text = tool_text(&result);
    assert!(text.contains("speak"), "members should list public methods:\n{text}");
    assert!(!text.contains("__repr__"), "members must exclude dunders without `all`:\n{text}");

    let all = session.call_tool(
        "members",
        &json!({ "symbols": ["Animal"], "file": "members_example.py", "all": true }),
    );
    assert!(
        tool_text(&all).contains("__repr__"),
        "members with `all` should include dunders:\n{}",
        tool_text(&all)
    );
}

#[test]
fn test_list() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let result = session.call_tool("list", &json!({ "file": "example.py" }));
    assert!(!is_error(&result), "list should succeed: {result}");
    let text = tool_text(&result);
    for expected in ["hello_world", "calculate_sum", "Calculator", "main"] {
        assert!(text.contains(expected), "list should include '{expected}':\n{text}");
    }
}

/// The agreed normalization is trailing-whitespace-trimmed byte equality: the
/// MCP bridge and the CLI must render the same characters for the same query.
#[test]
fn test_show_output_matches_cli() {
    common::require_ty();
    let mut session = McpSession::start(&workspace_root());

    let mcp_text = tool_text(&session.call_tool("show", &json!({ "symbols": ["Calculator"] })));
    let cli_text = cli_stdout(&["show", "Calculator"]);

    assert_eq!(
        mcp_text.trim_end(),
        cli_text.trim_end(),
        "MCP show output must be byte-identical to condensed CLI output"
    );
}

/// With no `--workspace`, the server resolves the workspace from its cwd —
/// the path harnesses actually take.
#[test]
fn test_workspace_defaults_to_cwd() {
    common::require_ty();
    let mut session = McpSession::start_in(&workspace_root(), None);

    let result = session.call_tool("show", &json!({ "symbols": ["hello_world"] }));
    assert!(!is_error(&result), "cwd-resolved workspace should work: {result}");
    assert!(tool_text(&result).contains("example.py:1:"));
}

/// Closing stdin is how a harness stops an MCP server; `tyf mcp` must exit
/// cleanly rather than hang.
#[test]
fn test_exits_cleanly_when_stdin_closes() {
    common::require_ty();
    let workspace = workspace_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tyf"))
        .arg("mcp")
        .arg("--workspace")
        .arg(&workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `tyf mcp`");

    drop(child.stdin.take().expect("stdin piped"));
    let status = child.wait().expect("failed to wait for `tyf mcp`");
    assert!(status.success(), "`tyf mcp` should exit 0 when stdin closes, got {status:?}");
}
