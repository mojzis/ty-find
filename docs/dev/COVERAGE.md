# Test Coverage Report

Generated: 2026-03-09 | Tool: cargo-tarpaulin | Overall: **32.8%** (849/2588 lines)

## Per-File Summary

| File | Covered | Total | Coverage | Status |
|------|---------|-------|----------|--------|
| `src/workspace/navigation.rs` | 30 | 30 | **100%** | Excellent |
| `src/daemon/pidfile.rs` | 38 | 40 | **95%** | Excellent |
| `src/cli/style.rs` | 32 | 37 | **86%** | Good |
| `src/debug.rs` | 81 | 105 | **77%** | Good |
| `src/ripgrep.rs` | 15 | 21 | **71%** | Good |
| `src/cli/output.rs` | 420 | 645 | **65%** | Partial |
| `src/workspace/detection.rs` | 10 | 16 | **62%** | Partial |
| `src/daemon/protocol.rs` | 33 | 62 | **53%** | Partial |
| `src/daemon/pool.rs` | 21 | 40 | **52%** | Partial |
| `src/daemon/client.rs` | 55 | 165 | **33%** | Low |
| `src/daemon/server.rs` | 66 | 415 | **16%** | Low |
| `src/commands.rs` | 48 | 578 | **8%** | Critical |
| `src/main.rs` | 0 | 84 | **0%** | None |
| `src/lsp/client.rs` | 0 | 151 | **0%** | None |
| `src/lsp/server.rs` | 0 | 48 | **0%** | None |
| `src/cli/generate_docs.rs` | 0 | 151 | **0%** | None |

## Untested Functions by Priority

### Priority 1: Zero Coverage (0%)

#### `src/main.rs`
- `main()` — entry point
- `run()` — main async orchestrator
- `dispatch_command()` — routes commands to handlers
- `resolve_workspace()` — workspace root detection from CLI flags
- `format_error_chain()` — anyhow error formatting

#### `src/lsp/client.rs`
- `TyLspClient::new()` — creates LSP client, starts response handler
- `initialize()` — LSP initialize handshake
- `open_document()` — textDocument/didOpen
- `goto_definition()` — definition at position
- `find_references()` — references at position
- `hover()` — hover information
- `workspace_symbols()` — fuzzy workspace symbol search
- `document_symbols()` — file outline
- `send_request()`, `send_notification()`, `send_message()`, `send_raw_message()` — RPC transport
- `start_response_handler()` — async response reader
- `file_uri()`, `parse_response_array()` — helpers

#### `src/lsp/server.rs`
- `TyCommand::build()` / `label()` — ty command construction
- `TyLspServer::resolve_ty_command()` — ty PATH resolution
- `start()` — spawn ty process
- `take_stdin()` / `take_stdout()` — stream extraction
- `shutdown()` / `Drop` — cleanup

#### `src/cli/generate_docs.rs`
- `generate_docs()` — markdown doc generation entry point
- `help_text()` — clap StyledStr → plain text
- `render_overview()` / `render_subcommand()` — markdown rendering
- `write_examples()` — example code blocks

### Priority 2: Nearly Untested (8-33%)

#### `src/commands.rs` (8%)
All main command handlers are untested:
- `handle_find_command()` — core find logic
- `handle_show_command()` — hover + definition + refs
- `handle_references_command()` — reference search
- `handle_members_command()` — class member listing
- `handle_document_symbols_command()` — file symbol listing
- `handle_daemon_command()` — daemon start/stop/status
- `connect_daemon()` — daemon connection setup
- `resolve_symbols_to_queries()` — symbol → position mapping
- `classify_and_resolve()` — query type detection
- `collect_queries()` — stdin/argv collection
- `execute_references_batch()` — batch RPC
- `enrich_references()` — add context to refs
- `find_name_column()` — column position in source
- `parse_file_position()` — "file:line:col" parsing

Only covered: `is_test_file()`, `dedup_locations()`, `partition_test_locations()`, `count_unique_files()` (incidental via output.rs tests).

#### `src/daemon/server.rs` (16%)
- `start()`, `bind_listeners()`, `write_pidfile()`, `spawn_accept_loops()` — server lifecycle
- `with_warmup()` — cold-start retry
- `send_error_response()` — error framing
- All ~15 RPC handler methods (`handle_hover`, `handle_definition`, etc.)

Only covered: `new()`, `get_socket_path()`, `ping_handler()`, `extract_member_signature()`.

#### `src/daemon/client.rs` (33%)
- `connect()`, `connect_with_timeout()` — connection establishment
- `send_request()`, `read_response()` — RPC transport
- All `execute_*()` methods — typed RPC wrappers
- `ensure_daemon_running()`, `spawn_daemon()` — auto-start logic

Only covered: `connect_with_pidfile()` TCP fallback path.

### Priority 3: Partially Tested (52-65%)

#### `src/daemon/pool.rs` (52%)
- `get_or_create()` — the critical async path (creates TyLspClient) is untested

#### `src/daemon/protocol.rs` (53%)
- All `DaemonError` constructors (`parse_error`, `invalid_request`, `method_not_found`, etc.)
- `Method::as_str()`

#### `src/cli/output.rs` (65%)
- `read_source_line()`, `read_definition_context()` — file reading
- `find_enclosing_symbol()`, `position_in_range()` — symbol tree walk
- `strip_code_fences()` — markdown cleanup
- CSV format, paths format, condensed detail paths

#### `src/workspace/detection.rs` (62%)
- `describe_detection()` — human-readable detection method
- Edge cases: no markers at filesystem root

## Testing Strategy Notes

- **LSP layer** (client.rs, server.rs): Requires either mock LSP server or integration tests with `ty` installed. Consider a `MockLspServer` that speaks JSON-RPC for unit testing.
- **Command handlers** (commands.rs): Could be tested with daemon mocks or by extracting pure logic from async handlers.
- **Daemon server handlers**: Could use in-process test harness that sends DaemonRequest and checks DaemonResponse.
- **generate_docs.rs**: Straightforward to test — call `generate_docs()` with a temp dir and verify markdown output.

## How to Reproduce

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --all-features --out json --output-dir /tmp/tyf-coverage --skip-clean
```
