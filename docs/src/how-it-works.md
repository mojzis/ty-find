<!-- Mermaid pitfalls (so you don't have to debug them again):
     - Timeline: colons in labels (e.g. "00:00") break parsing because ":"
       is the delimiter.  Use "0 sec", "2 sec" etc. instead.
     - Gantt + dateFormat X: "after taskId, 200" treats 200 as an absolute
       timestamp, not a duration.  Use explicit start/end: "taskId, 800, 1000".
     - Run `make lint-mermaid` to catch syntax errors before committing.  -->

# How It Works

ty-find is built as a three-layer system: a thin **CLI client**, a persistent **background daemon**, and the **ty LSP server** that does the actual Python analysis. This page explains how they fit together and why.

## Architecture overview

```mermaid
graph TB
    subgraph Terminal
        CLI["<b>tyf</b> CLI<br/><small>parse args · format output</small>"]
    end

    subgraph Daemon ["Daemon (background process)"]
        Router["Request Router"]
        subgraph Pool ["LSP Client Pool"]
            CA["TyLspClient A"]
            CB["TyLspClient B"]
        end
    end

    subgraph LSP ["ty LSP servers"]
        LA["ty lsp<br/><small>workspace A</small>"]
        LB["ty lsp<br/><small>workspace B</small>"]
    end

    CLI -- "JSON-RPC 2.0<br/>Unix socket" --> Router
    Router --> CA
    Router --> CB
    CA -- "LSP protocol<br/>stdin/stdout" --> LA
    CB -- "LSP protocol<br/>stdin/stdout" --> LB
```

Each layer has a single responsibility:

| Layer | Responsibility |
|-------|----------------|
| **CLI** (`tyf`) | Parse arguments, connect to daemon, format output |
| **Daemon** | Keep LSP servers alive between calls, route requests |
| **ty LSP** | Python type analysis, symbol resolution, indexing |

## Request lifecycle

Here's what happens when you run `tyf find calculate_sum`:

```mermaid
sequenceDiagram
    participant CLI as tyf CLI
    participant D as Daemon
    participant LSP as ty LSP server

    CLI->>D: 1. Connect (Unix socket)
    CLI->>D: 2. JSON-RPC "Definition" request

    Note over D: 3. Look up workspace<br/>in client pool (hit → reuse)

    D->>LSP: 4. textDocument/didOpen (if file not yet open)
    D->>LSP: 5. textDocument/definition
    LSP-->>D: 6. Location[]

    D-->>CLI: 7. JSON-RPC response

    Note over CLI: 8. Format & print results
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

```mermaid
gantt
    title Without daemon — cold start every time
    dateFormat X
    axisFormat %s

    section tyf find foo
    spawn ty lsp     :a1, 0, 800
    LSP initialize   :a2, 800, 1000
    index project    :a3, 1000, 3000
    LSP query        :a4, 3000, 3050
    format output    :a5, 3050, 3060

    section tyf find bar
    spawn ty lsp     :b1, 3060, 3860
    LSP initialize   :b2, 3860, 4060
    index project    :b3, 4060, 6060
    LSP query        :b4, 6060, 6110
    format output    :b5, 6110, 6120
```

```mermaid
gantt
    title With daemon — warm after first call
    dateFormat X
    axisFormat %s

    section tyf find foo
    connect to daemon :a1, 0, 20
    send request      :a2, 20, 30
    LSP query         :a3, 30, 430
    format output     :a4, 430, 440

    section tyf find bar
    connect to daemon :b1, 440, 460
    send request      :b2, 460, 470
    LSP query         :b3, 470, 900
    format output     :b4, 900, 910
```

### Auto-start and version checking

The CLI automatically manages the daemon lifecycle:

```mermaid
flowchart TD
    A["tyf find foo"] --> B{"Is daemon<br/>running?"}
    B -- No --> C["Spawn daemon"]
    C --> D["Wait for ready"]
    D --> G["Send request"]
    B -- Yes --> E["Ping daemon"]
    E --> F{"Version<br/>matches?"}
    F -- Yes --> G
    F -- No --> H["Stop old daemon"]
    H --> C
```

When you upgrade ty-find, the CLI detects that the running daemon is from an older version and restarts it automatically.

### Idle shutdown

The daemon tracks activity at two levels:

- **Per-workspace**: Each LSP client records its last access time. Clients idle for more than 5 minutes are cleaned up (the `ty lsp` process is terminated).
- **Daemon-wide**: If all workspace clients are idle, the daemon shuts itself down.

```mermaid
timeline
    title Daemon lifecycle example
    0 sec : tyf find foo
          : Daemon starts
          : Workspace A client created
    2 sec : tyf refs bar
          : Workspace A client reused
    30 sec : tyf find baz
           : Workspace A client reused
           : Idle timer reset
    5 min 30 sec : No activity for 5 min
                 : Workspace A client removed
                 : No clients remain
                 : Daemon shuts down
```

## LSP client pool

The daemon maintains a pool of LSP clients, one per workspace. When a request arrives, the daemon resolves it to a workspace root and looks up the corresponding client.

```mermaid
flowchart TD
    R["Incoming request<br/><code>workspace: /home/user/project</code>"] --> L{"Lookup workspace<br/>in HashMap<br/><small>(lock held)</small>"}
    L -- Hit --> RET["Return existing client"]
    L -- Miss --> REL["Release lock"]
    REL --> SPAWN["Spawn ty lsp<br/>Initialize LSP<br/><small>(async, no lock held)</small>"]
    SPAWN --> RELOCK["Re-acquire lock"]
    RELOCK --> CHECK{"Check again<br/><small>(another task may<br/>have created it)</small>"}
    CHECK -- Already exists --> RET
    CHECK -- Still missing --> INS["Insert new client"] --> RET
```

The pool uses a **lock-free fast path** pattern: the `std::sync::Mutex` is held only for the HashMap lookup (microseconds), then dropped before any async work. This avoids holding a lock across `.await`, which would block other tasks.

## Communication protocols

### CLI ↔ Daemon: JSON-RPC 2.0 over Unix socket

The CLI and daemon communicate using JSON-RPC 2.0 with LSP-style message framing:

```
Content-Length: 128\r\n
\r\n
{"jsonrpc":"2.0","id":1,"method":"Definition","params":{...}}
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

```mermaid
sequenceDiagram
    participant D as Daemon
    participant LSP as ty lsp (child process)

    D->>LSP: stdin: LSP request (JSON-RPC 2.0)
    LSP-->>D: stdout: LSP response
```

Response routing works through request IDs: each outgoing request gets a unique integer ID (from an `AtomicU64`). A background task reads responses from stdout and matches them to pending requests using a `HashMap<u64, oneshot::Sender>`.

```mermaid
sequenceDiagram
    participant Caller as send_request()
    participant Map as pending_requests
    participant Stdin as stdin (to ty)
    participant Handler as response_handler
    participant Stdout as stdout (from ty)

    Caller->>Map: store tx for id=42
    Caller->>Stdin: write JSON-RPC {id: 42, ...}
    Note over Caller: await rx...

    Stdout-->>Handler: read {id: 42, result: ...}
    Handler->>Map: remove(42) → tx
    Handler->>Caller: tx.send(response)
    Note over Caller: unblocked!
```

## Concurrency model

All parallelism is handled by the daemon, not the CLI:

- The LSP protocol runs over a single stdin/stdout pipe per server, so requests are inherently sequential.
- Multi-symbol operations (like `tyf inspect A B C`) are sent as a single batch RPC call. The daemon processes them sequentially on its LSP client and returns merged results.
- The CLI never spawns multiple connections or concurrent requests. This keeps the architecture simple and avoids race conditions.

```mermaid
sequenceDiagram
    participant CLI as tyf CLI
    participant D as Daemon
    participant LSP as ty LSP

    CLI->>D: inspect [A, B, C]
    D->>LSP: hover(A)
    LSP-->>D: result A
    D->>LSP: hover(B)
    LSP-->>D: result B
    D->>LSP: hover(C)
    LSP-->>D: result C
    D-->>CLI: [results for A, B, C]
```

## Document tracking

The LSP protocol requires that a client sends `textDocument/didOpen` before querying a file, and only sends it once per file per session. The LSP client tracks opened documents in a `HashSet<String>`:

```mermaid
flowchart TD
    Q["Query for src/main.py:10:5"] --> C{"URI in<br/>opened_documents?"}
    C -- No --> O["Send didOpen"]
    O --> ADD["Add URI to set"]
    ADD --> DEF["Send textDocument/definition"]
    C -- Yes --> DEF
```

Sending a duplicate `didOpen` would cause the LSP server to re-analyze the file, returning null results during the re-analysis window. The tracking set prevents this.

## Warmup and retries

On a cold start, the LSP server may not be fully ready to answer queries even after initialization completes. The daemon handles this with automatic retries:

```mermaid
sequenceDiagram
    participant D as Daemon
    participant LSP as ty LSP

    Note over D,LSP: First query after daemon start

    D->>LSP: hover(symbol)
    LSP-->>D: null (still indexing)
    Note over D: wait 100ms

    D->>LSP: hover(symbol)
    LSP-->>D: null (still not ready)
    Note over D: wait 200ms

    D->>LSP: hover(symbol)
    LSP-->>D: result ✓
    Note over D: return result
```

Retries use exponential backoff (200ms, 400ms, 800ms, 1600ms) and apply to all operations that can return empty or null results during warmup, including `hover`, `workspace/symbol`, `definition`, `references`, and `documentSymbol`.
