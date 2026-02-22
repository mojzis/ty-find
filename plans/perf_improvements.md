# Performance Improvements: `inspect` Timeout on Large Codebases

## Problem

`inspect` can timeout against large codebases due to three compounding issues:

### 1. Hard 5-second timeout on every operation

`src/daemon/client.rs:25` — `DEFAULT_TIMEOUT = 5s` wraps every daemon request. On large codebases, ty's LSP server can easily exceed this for:
- `workspace/symbol` queries (scanning thousands of files)
- `textDocument/references` (finding all usages across the project)
- First requests after LSP initialization (ty is still indexing in the background)

### 2. Sequential execution with new connections per operation

`src/main.rs:256-346` — `inspect_single_symbol` creates a **new `DaemonClient::connect()`** for each operation: one for definitions (line 280), one for hover (line 329), one for references (line 335). All run sequentially.

### 3. Symbols processed sequentially

`src/main.rs:232-235` — the outer loop in `handle_inspect_command` processes symbols one at a time with no parallelism.

## Proposed Fixes

### A. Make timeout configurable (high impact, low effort)

- Increase `DEFAULT_TIMEOUT` to something more reasonable (e.g. 30s)
- Add a `--timeout <seconds>` CLI flag so users can tune for their codebase
- Consider separate timeouts for different operation types (workspace/symbol needs more time than hover)

### B. Parallelize hover + references within `inspect_single_symbol` (high impact, medium effort)

Steps 2 and 3 (hover and references) are independent — run them concurrently with `tokio::join!` instead of sequentially. This alone would cut per-symbol time roughly in half.

### C. Parallelize across symbols in `handle_inspect_command` (high impact, medium effort)

Use `futures::future::join_all` or similar to process multiple symbols concurrently instead of the sequential `for` loop. Combined with (B), inspecting 3 symbols would go from ~9 serial requests to ~3 parallel batches.

### D. Reuse daemon connection (medium impact, low effort)

Currently each operation opens a new Unix socket connection. A single connection could be reused for all operations within one `inspect_single_symbol` call (and potentially across symbols).

## Notes

- A big refactor is ongoing in another branch — wait to see how that affects perf and code structure before implementing these
- The daemon's `LspClientPool` already caches ty LSP server instances per workspace, so warm starts are fast; the bottleneck is the client-side connection and timeout handling
