# How It Works

ty-find is built as a three-layer system: a thin **CLI client**, a persistent **background daemon**, and the **ty LSP server** that does the actual Python analysis. This page explains how they fit together and why.

## Architecture overview

```
 ┌─────────────────────────────────────────────────────────────────┐
 │  Terminal                                                       │
 │                                                                 │
 │  $ tyf inspect MyClass                                          │
 │  $ tyf find calculate_sum --fuzzy                               │
 │  $ tyf refs handle_request                                      │
 │                                                                 │
 └──────────────────────────┬──────────────────────────────────────┘
                            │
                    JSON-RPC 2.0 over
                    Unix domain socket
                            │
 ┌──────────────────────────▼──────────────────────────────────────┐
 │  Daemon  (background process)                                   │
 │                                                                 │
 │  ┌───────────────────────────────────────────────────────────┐  │
 │  │  LSP Client Pool                                          │  │
 │  │                                                           │  │
 │  │   workspace A  ──▶  TyLspClient  ──▶  ty lsp (process)   │  │
 │  │   workspace B  ──▶  TyLspClient  ──▶  ty lsp (process)   │  │
 │  │   ...                                                     │  │
 │  └───────────────────────────────────────────────────────────┘  │
 │                                                                 │
 └─────────────────────────────────────────────────────────────────┘
```

Each layer has a single responsibility:

| Layer | Responsibility |
|-------|----------------|
| **CLI** (`tyf`) | Parse arguments, connect to daemon, format output |
| **Daemon** | Keep LSP servers alive between calls, route requests |
| **ty LSP** | Python type analysis, symbol resolution, indexing |

## Request lifecycle

Here's what happens when you run `tyf find calculate_sum`:

```
  CLI                      Daemon                    ty LSP server
   │                         │                            │
   │  1. Connect to          │                            │
   │     Unix socket         │                            │
   ├────────────────────────▶│                            │
   │                         │                            │
   │  2. Send JSON-RPC       │                            │
   │     "Definition" req    │                            │
   ├────────────────────────▶│                            │
   │                         │  3. Look up workspace      │
   │                         │     in client pool         │
   │                         │     (hit → reuse client)   │
   │                         │                            │
   │                         │  4. textDocument/didOpen    │
   │                         │     (if file not yet open)  │
   │                         ├───────────────────────────▶│
   │                         │                            │
   │                         │  5. textDocument/definition │
   │                         ├───────────────────────────▶│
   │                         │                            │
   │                         │  6. Location[]             │
   │                         │◀───────────────────────────┤
   │                         │                            │
   │  7. JSON-RPC response   │                            │
   │◀────────────────────────┤                            │
   │                         │                            │
   │  8. Format & print      │                            │
   │     results             │                            │
   ▼                         │                            │
```

Steps 1–8 take **50–100 ms** on a warm daemon. Without the daemon, every call would pay the full LSP startup cost (several seconds).

## The daemon

The daemon is a long-running background process that listens on a Unix domain socket at `/tmp/ty-find-{uid}.sock`. It starts automatically on first use and shuts itself down after 5 minutes of inactivity.

### Why a daemon?

Starting an LSP server is expensive. The ty LSP process needs to:

1. Spawn and initialize
2. Index the Python project (parse files, resolve imports, build type information)
3. Reach a "ready" state where it can answer queries

This takes **1–5 seconds** depending on project size. The daemon pays this cost once and keeps the server running for subsequent calls.

```
  Without daemon (cold every time)        With daemon (warm after first call)
  ─────────────────────────────────       ──────────────────────────────────

  $ tyf find foo                          $ tyf find foo
  ├── spawn ty lsp ........... 800ms      ├── connect to daemon .... 2ms
  ├── LSP initialize ........ 200ms       ├── send request ......... 1ms
  ├── index project ......... 2000ms      ├── LSP query ............ 40ms
  ├── LSP query ............. 50ms        └── format output ........ 1ms
  └── format output ......... 1ms               Total: ~50ms
        Total: ~3000ms
                                          (LSP server already running
  $ tyf find bar                           and project already indexed)
  ├── spawn ty lsp ........... 800ms
  ├── LSP initialize ........ 200ms
  ├── index project ......... 2000ms      $ tyf find bar
  ├── LSP query ............. 50ms        ├── connect to daemon .... 2ms
  └── format output ......... 1ms         ├── send request ......... 1ms
        Total: ~3000ms again              ├── LSP query ............ 40ms
                                          └── format output ........ 1ms
                                                Total: ~50ms again
```

### Auto-start and version checking

The CLI automatically manages the daemon lifecycle:

```
  tyf find foo
   │
   ├── Is daemon running?
   │    ├── No  → spawn daemon, wait for ready, then proceed
   │    └── Yes → ping daemon
   │              ├── Version matches?  → proceed
   │              └── Version mismatch? → stop old daemon, start new one
   │
   └── Send request to daemon
```

When you upgrade ty-find, the CLI detects that the running daemon is from an older version and restarts it automatically.

### Idle shutdown

The daemon tracks activity at two levels:

- **Per-workspace**: Each LSP client records its last access time. Clients idle for more than 5 minutes are cleaned up (the `ty lsp` process is terminated).
- **Daemon-wide**: If all workspace clients are idle, the daemon shuts itself down.

```
  Time ──────────────────────────────────────────────────────▶

  00:00  tyf find foo        (daemon starts, workspace A client created)
  00:02  tyf refs bar        (workspace A client reused)
  00:30  tyf find baz        (workspace A client reused, timer reset)
  05:30  [no activity]       (workspace A idle > 5 min → client removed)
  05:30  [no clients]        (daemon shuts down)
```

## LSP client pool

The daemon maintains a pool of LSP clients, one per workspace. When a request arrives, the daemon resolves it to a workspace root and looks up the corresponding client.

```
  Incoming request:  { workspace: "/home/user/my-project", ... }
                                │
                                ▼
                    ┌───────────────────────┐
                    │   LSP Client Pool     │
                    │                       │
                    │   Fast path (locked): │
                    │   lookup workspace    │──── Hit? ──▶ return client
                    │   in HashMap          │
                    │                       │
                    │   Miss?               │
                    │   └── release lock    │
                    │       spawn ty lsp    │  ◀── async, no lock held
                    │       initialize LSP  │
                    │       re-lock         │
                    │       └── check again │  ◀── another task may have
                    │           insert      │      created it meanwhile
                    │           return      │
                    └───────────────────────┘
```

The pool uses a **lock-free fast path** pattern: the `std::sync::Mutex` is held only for the HashMap lookup (microseconds), then dropped before any async work. This avoids holding a lock across `.await`, which would block other tasks.

## Communication protocols

### CLI ↔ Daemon: JSON-RPC 2.0 over Unix socket

The CLI and daemon communicate using JSON-RPC 2.0 with LSP-style message framing:

```
Content-Length: 128\r\n
\r\n
{"jsonrpc":"2.0","id":1,"method":"Definition","params":{"workspace":"/home/user/project","file":"src/main.py","line":10,"character":5}}
```

Available RPC methods:

| Method | Description |
|--------|-------------|
| `Ping` | Health check (returns version and uptime) |
| `Shutdown` | Gracefully stop the daemon |
| `Definition` | Go to definition of a symbol at a position |
| `Hover` | Get type information for a symbol at a position |
| `References` | Find all references to a symbol |
| `BatchReferences` | Find references for multiple symbols in one call |
| `WorkspaceSymbols` | Search for symbols by name across the workspace |
| `DocumentSymbols` | List all symbols in a file |
| `Inspect` | Combined definition + hover + references |
| `Members` | Public interface of a class |
| `Diagnostics` | Type errors in a file |

### Daemon ↔ ty LSP: LSP protocol over stdin/stdout

The daemon communicates with each `ty lsp` process using the standard [Language Server Protocol](https://microsoft.github.io/language-server-protocol/). Messages use the same `Content-Length` framing but carry standard LSP methods like `textDocument/definition` and `textDocument/hover`.

```
  Daemon                                   ty lsp (child process)
   │                                            │
   │  ─── stdin ──────────────────────────────▶ │
   │       LSP requests (JSON-RPC 2.0)          │
   │                                            │
   │  ◀── stdout ─────────────────────────────  │
   │       LSP responses                        │
   │                                            │
```

Response routing works through request IDs: each outgoing request gets a unique integer ID (from an `AtomicU64`). A background task reads responses from stdout and matches them to pending requests using a `HashMap<u64, oneshot::Sender>`.

```
  send_request("textDocument/definition", params)
   │
   ├── id = next_id.fetch_add(1)          // 42
   ├── pending_requests[42] = tx          // store sender
   ├── write to stdin                     // send to ty
   └── await rx                           // wait for response

  response_handler (background task)
   │
   ├── read stdout: {"id": 42, ...}
   ├── sender = pending_requests.remove(42)
   └── sender.send(response)              // unblocks await above
```

## Concurrency model

All parallelism is handled by the daemon, not the CLI:

- The LSP protocol runs over a single stdin/stdout pipe per server, so requests are inherently sequential.
- Multi-symbol operations (like `tyf inspect A B C`) are sent as a single batch RPC call. The daemon processes them sequentially on its LSP client and returns merged results.
- The CLI never spawns multiple connections or concurrent requests. This keeps the architecture simple and avoids race conditions.

```
  CLI                          Daemon
   │                             │
   │  inspect [A, B, C]         │
   ├────────────────────────────▶│
   │                             ├── hover(A)  ──▶  ty lsp
   │                             ├── hover(B)  ──▶  ty lsp
   │                             ├── hover(C)  ──▶  ty lsp
   │                             │
   │  [results for A, B, C]     │
   │◀────────────────────────────┤
   │                             │
```

## Document tracking

The LSP protocol requires that a client sends `textDocument/didOpen` before querying a file, and only sends it once per file per session. The LSP client tracks opened documents in a `HashSet<String>`:

```
  query for "src/main.py:10:5"
   │
   ├── Is "file:///path/to/src/main.py" in opened_documents?
   │    ├── No  → send didOpen, add to set, then query
   │    └── Yes → query directly (already open)
   │
   └── send textDocument/definition
```

Sending a duplicate `didOpen` would cause the LSP server to re-analyze the file, returning null results during the re-analysis window. The tracking set prevents this.

## Warmup and retries

On a cold start, the LSP server may not be fully ready to answer queries even after initialization completes. The daemon handles this with automatic retries:

```
  First query after daemon start
   │
   ├── attempt 1: hover(symbol) → null       (server still indexing)
   │   └── wait 100ms
   ├── attempt 2: hover(symbol) → null       (still not ready)
   │   └── wait 200ms
   ├── attempt 3: hover(symbol) → result!    (server ready)
   │
   └── return result
```

Retries use exponential backoff (100ms, 200ms, 400ms) and only apply to operations that can return null during warmup (like `hover` and `workspace/symbol`). Definition lookups don't need retries because they return empty results rather than null when the server isn't ready.
