# Architecture

## Key Architectural Patterns

**LSP Communication Flow**:
- `TyLspServer` spawns and manages the `ty lsp` process
- `TyLspClient` handles JSON-RPC protocol with initialization, requests, and response parsing
- Communication is async using tokio with proper message framing (Content-Length headers)

**Dual Build System**:
- `Cargo.toml` defines the Rust binary with CLI dependencies (clap, tokio, serde)
- `pyproject.toml` uses maturin backend (`bindings = "bin"`) to package the Rust binary as a Python wheel

**Command Processing**:
- Main commands: `show` (all-in-one), `find` (definitions), `refs` (references), `members` (class interface), `calls` (call hierarchy), `list` (file outline)
- `find` supports `--fuzzy` for partial/prefix matching via workspace symbols
- `find`, `show`, `refs`, `members`, and `calls` accept multiple symbols in one call to reduce tool invocations (results grouped by symbol)
- `calls` walks the call graph entirely in the daemon (`CallWalker` in `src/daemon/server.rs`) and returns a finished tree; the CLI issues one RPC, never per-level requests. Depth is capped there, not in the CLI
- `SymbolFinder` does text-based symbol matching with whole-word detection
- `OutputFormatter` supports multiple formats: human, JSON, CSV, paths-only

**Two frontends, one bridge**:
- The CLI and the MCP server (`tyf mcp`, `src/mcp/`) are peers. Both parse a request, call the same `handle_*_command` in `src/commands.rs`, and render the result; neither does LSP work of its own. The MCP surface covers the five lookup commands; `calls` is CLI-only while it remains gated on `ty >= 0.0.41`.
- Command handlers return a `CommandOutput { stdout, stderr }` rather than printing. The CLI calls `emit()`; the MCP bridge calls `combined()` and returns it as text content. There is one renderer, so tool output is byte-identical (modulo trailing whitespace) to `tyf --detail condensed --color never`.
- The MCP server adds no daemon-side logic, no retries, and no protocol of its own: it is stdio in, existing daemon RPC out. One warm index serves both frontends.
- `tyf mcp` owns stdout for the JSON-RPC stream, so nothing else may write there — `--verbose` logging is routed to stderr for that subcommand.
- rmcp serves each tool call on its own task, so a harness can pipeline calls. LSP work is still serialized by the daemon, but daemon *startup* is not idempotent, so `ensure_daemon_running` takes a process-wide `tokio::sync::Mutex`: without it, concurrent first calls each spawn a daemon and each new daemon unlinks the previous one's socket on bind, orphaning the losers.

**ty capability gating**:
- `tyf` binds to whatever `ty` is installed, and `ty` is `0.0.x`. Where a feature is not present across the whole supported range, the LSP client records the relevant `initialize` capability and the daemon returns a structured error rather than an empty result — an empty result is indistinguishable from a legitimate "nothing found".
- Today this applies to one feature: `callHierarchyProvider` (`ty >= 0.0.41`), which gates `calls`. See `docs/dev/TY_VERSIONS.md` and `docs/dev/call-hierarchy-spike.md`.

**Concurrency rule — daemon handles all parallelism**:
- All multi-query operations (batch references, multi-symbol show, etc.) must be batched into a single RPC call and processed by the daemon, **not** parallelized on the CLI client side.
- The ty LSP server communicates through a single stdin/stdout pipe, so LSP requests are inherently sequential. Spawning parallel client connections only adds connection overhead without concurrency benefit.
- Use `BatchReferences` (or similar batch RPC methods) to send multiple queries in one call. The daemon processes them sequentially on the shared LSP client and returns merged results.

## Python Integration Strategy

The project uses maturin to bridge Rust and Python ecosystems:
- Rust binary provides performance for LSP communication
- Python packaging allows `pip install` and `uv sync` integration
- Users add `ty-find @ git+https://github.com/user/ty-find.git` to pyproject.toml
- maturin automatically builds Rust binary during Python package installation

## Ripgrep Circuit-Breaker in Retry Flow

When an LSP operation returns empty/null results (e.g., workspace symbol lookup for a non-existent symbol), the daemon normally retries with exponential back-off (200ms, 400ms, 800ms, 1600ms) to allow the LSP server to finish indexing.

To avoid wasting ~3 seconds on symbols that genuinely don't exist, the retry loop uses `rg` (ripgrep) as a fast negative filter after the first empty result:

1. First LSP attempt returns empty → run `rg --count --word-regexp --fixed-strings --type py '{symbol}' {workspace_root}`
2. If `rg` exit code 1 (no matches) → symbol provably doesn't exist, skip all retries
3. If `rg` exit code 0 (matches found) → symbol might exist, continue retries as normal
4. If `rg` not found or errors → graceful fallback, continue retries as normal

This is a **one-directional optimization**: `rg` returning zero matches guarantees non-existence. `rg` returning matches does NOT guarantee the symbol exists (it could be in a comment or string), so retries continue in that case.

The check is implemented in `src/ripgrep.rs` and integrated into the `with_warmup()` function in `src/daemon/server.rs`. Currently applied to workspace symbol lookups where a symbol name is available; position-based operations (hover, definition, references) pass `None` to skip the check.

## Dependencies

- **ty LSP server** must be available in PATH or via `uvx` (users install via `uv add --dev ty`)
- **ripgrep** (`rg`) — optional, used as a circuit-breaker to speed up non-existent symbol lookups
- **Rust toolchain** required for building from source
- **tokio** for async LSP communication and process management
- **clap** for CLI parsing with subcommands and multiple output formats
- **rmcp** (pinned exactly) + **schemars** for the MCP server frontend and its tool schemas
