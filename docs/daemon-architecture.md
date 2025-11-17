# Daemon Architecture Design

**Date**: 2025-11-17
**Status**: Implementation Phase

## Overview

This document describes the daemon architecture for ty-find, which enables fast CLI responses by maintaining persistent LSP server connections in the background.

## Goals

1. **Transparent**: User never needs to think about daemon - it auto-starts
2. **Fast**: <100ms response time for warm cache
3. **Reliable**: Handle crashes gracefully, auto-restart if needed
4. **Simple**: Unix socket for communication (Linux/Mac)
5. **Efficient**: One LSP connection per workspace, shared across commands

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        ty-find CLI                          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  1. Check if daemon is running                      │   │
│  │  2. If not, spawn daemon in background              │   │
│  │  3. Connect to daemon via Unix socket               │   │
│  │  4. Send JSON-RPC request                           │   │
│  │  5. Receive response                                │   │
│  │  6. Format and display output                       │   │
│  └─────────────────────────────────────────────────────┘   │
└──────────────────────┬──────────────────────────────────────┘
                       │ Unix Socket
                       │ /tmp/ty-find-{user}.sock
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                     ty-find Daemon                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         LSP Client Pool (per workspace)              │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐    │  │
│  │  │Workspace A │  │Workspace B │  │Workspace C │    │  │
│  │  │TyLspClient │  │TyLspClient │  │TyLspClient │    │  │
│  │  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘    │  │
│  └────────┼───────────────┼───────────────┼───────────┘  │
│           │               │               │               │
│  ┌────────▼───────────────▼───────────────▼───────────┐  │
│  │              Response Cache (LRU)                   │  │
│  │  Key: (workspace, command, params)                  │  │
│  │  Value: (result, timestamp)                         │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         Idle Timeout Manager                         │  │
│  │  - Track last activity time                          │  │
│  │  - Shutdown after 5 minutes idle                     │  │
│  │  - Clean up LSP connections gracefully               │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────────┘
                       │
                 ┌─────▼──────┐
                 │  ty lsp    │ (one per workspace)
                 │  processes │
                 └────────────┘
```

## Communication Protocol

### Socket Location

- Linux/Mac: `/tmp/ty-find-{uid}.sock`
- Windows: `\\.\pipe\ty-find-{uid}` (named pipe)

Where `{uid}` is the user ID to ensure isolation between users.

### JSON-RPC Protocol

We use JSON-RPC 2.0 for communication between CLI and daemon.

#### Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "hover",
  "params": {
    "workspace": "/path/to/workspace",
    "file": "/path/to/file.py",
    "line": 10,
    "column": 5
  }
}
```

#### Response Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "symbol": "foo",
    "type": "str",
    "documentation": "A foo function",
    "signature": "def foo() -> str:"
  }
}
```

#### Error Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32000,
    "message": "File not found",
    "data": {
      "file": "/path/to/file.py"
    }
  }
}
```

### Supported Methods

1. **hover** - Get type information and documentation
2. **definition** - Go to definition
3. **workspace_symbols** - Search symbols across workspace
4. **document_symbols** - Get outline of file
5. **diagnostics** - Get type errors and warnings
6. **ping** - Health check
7. **shutdown** - Graceful shutdown

## File Structure

```
src/
├── daemon/
│   ├── mod.rs              # Public interface
│   ├── server.rs           # Daemon server implementation
│   ├── client.rs           # Daemon client (CLI side)
│   ├── protocol.rs         # JSON-RPC types
│   ├── pool.rs             # LSP client pool management
│   └── cache.rs            # Response caching
├── lsp/
│   ├── mod.rs
│   ├── client.rs           # Enhanced with new methods
│   ├── server.rs
│   └── protocol.rs         # Extended LSP types
├── cli/
│   ├── mod.rs
│   ├── args.rs             # Updated with new commands
│   └── output.rs           # JSON output formatting
└── main.rs                 # Updated to use daemon
```

## Implementation Plan

### Phase 1: Daemon Protocol (Current)

**Files to create/modify**:
- `src/daemon/mod.rs` - Public API
- `src/daemon/protocol.rs` - JSON-RPC types
- `src/daemon/server.rs` - Daemon server skeleton
- `src/daemon/client.rs` - Daemon client skeleton

**Key types**:
```rust
// JSON-RPC request/response types
pub struct DaemonRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

pub struct DaemonResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<DaemonError>,
}

// Daemon server
pub struct DaemonServer {
    socket_path: PathBuf,
    lsp_pool: Arc<Mutex<LspClientPool>>,
    cache: Arc<Mutex<ResponseCache>>,
    shutdown_tx: broadcast::Sender<()>,
}

// Daemon client
pub struct DaemonClient {
    socket_path: PathBuf,
    stream: Option<UnixStream>,
}
```

### Phase 2: LSP Client Pool

**Files to create**:
- `src/daemon/pool.rs`

**Key functionality**:
```rust
pub struct LspClientPool {
    clients: HashMap<PathBuf, Arc<TyLspClient>>,
    last_access: HashMap<PathBuf, Instant>,
}

impl LspClientPool {
    pub async fn get_or_create(&mut self, workspace: PathBuf)
        -> Result<Arc<TyLspClient>> {
        // Return existing or create new LSP client
    }

    pub async fn cleanup_idle(&mut self, timeout: Duration) {
        // Remove idle clients
    }
}
```

### Phase 3: Response Cache

**Files to create**:
- `src/daemon/cache.rs`

**Key functionality**:
```rust
pub struct ResponseCache {
    cache: LruCache<CacheKey, CacheEntry>,
}

#[derive(Hash, Eq, PartialEq)]
struct CacheKey {
    workspace: PathBuf,
    method: String,
    params_hash: u64,
}

struct CacheEntry {
    result: Value,
    timestamp: Instant,
}
```

### Phase 4: Enhanced LSP Features

**Files to modify**:
- `src/lsp/client.rs` - Add hover, workspace_symbols, etc.
- `src/lsp/protocol.rs` - Add new LSP types

**New methods to add**:
```rust
impl TyLspClient {
    pub async fn hover(&self, file: &str, line: u32, char: u32)
        -> Result<Option<Hover>> { ... }

    pub async fn workspace_symbols(&self, query: &str)
        -> Result<Vec<SymbolInformation>> { ... }

    pub async fn document_symbols(&self, file: &str)
        -> Result<Vec<DocumentSymbol>> { ... }
}
```

### Phase 5: CLI Integration

**Files to modify**:
- `src/main.rs` - Use daemon client
- `src/cli/args.rs` - Add daemon subcommand
- `src/cli/output.rs` - Enhanced JSON formatting

**New CLI structure**:
```rust
#[derive(Subcommand)]
pub enum Commands {
    Definition { ... },
    Find { ... },
    Interactive { ... },
    Hover { ... },              // New
    WorkspaceSymbols { ... },   // New
    DocumentSymbols { ... },    // New
    Diagnostics { ... },        // New
    Daemon {                     // New
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    Start,
    Stop,
    Status,
}
```

## Daemon Lifecycle

### Auto-Start Flow

1. User runs: `ty-find hover file.py --line 10 --column 5`
2. CLI checks if daemon socket exists
3. If no socket:
   - Spawn daemon process in background
   - Daemon creates socket and starts listening
   - Wait up to 1 second for socket to appear
4. CLI connects to socket
5. CLI sends request and waits for response
6. CLI formats and displays response

### Daemon Startup

```rust
async fn start_daemon() -> Result<()> {
    // 1. Create socket
    let socket_path = get_socket_path()?;
    let listener = UnixListener::bind(&socket_path)?;

    // 2. Daemonize (fork and detach)
    if !cfg!(windows) {
        daemonize()?;
    }

    // 3. Accept connections in loop
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(handle_connection(stream));
    }
}
```

### Idle Timeout

```rust
async fn idle_timeout_task(last_activity: Arc<Mutex<Instant>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let idle = {
            let last = last_activity.lock().unwrap();
            last.elapsed()
        };

        if idle > Duration::from_secs(300) {  // 5 minutes
            tracing::info!("Idle timeout, shutting down");
            shutdown().await;
            break;
        }
    }
}
```

### Graceful Shutdown

```rust
async fn shutdown() -> Result<()> {
    // 1. Broadcast shutdown signal
    shutdown_tx.send(())?;

    // 2. Close all LSP clients
    for (workspace, client) in lsp_pool.clients {
        client.shutdown().await?;
    }

    // 3. Remove socket file
    fs::remove_file(&socket_path)?;

    Ok(())
}
```

## Error Handling

### Client-Side

```rust
async fn execute_via_daemon(cmd: Commands) -> Result<()> {
    // Try to connect to daemon
    match DaemonClient::connect().await {
        Ok(client) => {
            // Send request to daemon
            client.execute(cmd).await
        }
        Err(_) => {
            // Daemon not running, try to start it
            DaemonServer::spawn_background()?;

            // Wait for daemon to start (with timeout)
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if let Ok(client) = DaemonClient::connect().await {
                    return client.execute(cmd).await;
                }
            }

            // Fall back to direct execution
            tracing::warn!("Daemon failed to start, executing directly");
            execute_directly(cmd).await
        }
    }
}
```

### Server-Side

```rust
async fn handle_request(req: DaemonRequest) -> DaemonResponse {
    match process_request(req).await {
        Ok(result) => DaemonResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(result),
            error: None,
        },
        Err(e) => DaemonResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: None,
            error: Some(DaemonError {
                code: -32000,
                message: e.to_string(),
                data: None,
            }),
        }
    }
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_daemon_client_connect() { ... }

    #[tokio::test]
    async fn test_lsp_pool_get_or_create() { ... }

    #[tokio::test]
    async fn test_cache_hit() { ... }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_daemon_auto_start() {
    // Ensure no daemon is running
    stop_daemon().await;

    // Execute command
    let output = Command::cargo_bin("ty-find")
        .arg("hover")
        .arg("test.py")
        .arg("--line").arg("1")
        .arg("--column").arg("1")
        .output()
        .await?;

    // Verify daemon was started
    assert!(daemon_is_running());
}

#[tokio::test]
async fn test_daemon_performance() {
    // First call (cold start)
    let start = Instant::now();
    execute_hover().await;
    let cold_time = start.elapsed();

    // Second call (warm)
    let start = Instant::now();
    execute_hover().await;
    let warm_time = start.elapsed();

    // Warm should be much faster
    assert!(warm_time < cold_time / 5);
}
```

## Performance Targets

### Metrics

| Operation | Without Daemon | With Daemon (Cold) | With Daemon (Warm) | Goal |
|-----------|---------------|-------------------|-------------------|------|
| hover | 1-2s | 1-2s | 50-100ms | <100ms |
| definition | 1-2s | 1-2s | 50-100ms | <100ms |
| workspace_symbols | 2-3s | 2-3s | 100-200ms | <200ms |
| Cache hit | N/A | N/A | 5-10ms | <10ms |

### Optimization Strategies

1. **Lazy initialization**: Only start LSP server when first request for workspace arrives
2. **Connection reuse**: Keep LSP connections alive between requests
3. **Response caching**: Cache immutable results (definitions, symbols)
4. **Preloading**: Predict likely next requests and preload data

## Security Considerations

1. **Socket permissions**: Socket file should be readable/writable only by owner
   ```rust
   let socket_path = format!("/tmp/ty-find-{}.sock", users::get_current_uid());
   ```

2. **Process isolation**: Each user has their own daemon
3. **No network exposure**: Unix socket only, no TCP
4. **Input validation**: Validate all paths and parameters

## Deployment

### Installation

```bash
# Install via pip (includes Rust binary)
pip install ty-find

# Daemon starts automatically on first use
ty-find hover myfile.py --line 10 --column 5

# Optional: pre-start daemon
ty-find daemon start
```

### Configuration

```toml
# ~/.config/ty-find/config.toml
[daemon]
idle_timeout = 300  # seconds
cache_size = 1000   # number of cached responses
auto_start = true   # auto-start daemon if not running

[performance]
max_lsp_clients = 10  # max concurrent workspaces
cache_ttl = 60        # cache entry TTL in seconds
```

## Monitoring and Debugging

### Daemon Status

```bash
$ ty-find daemon status
Daemon: running
PID: 12345
Uptime: 5m 23s
Active workspaces: 2
  - /home/user/project1 (last used: 30s ago)
  - /home/user/project2 (last used: 2m ago)
Cache: 45 entries
Memory: 85 MB
```

### Verbose Logging

```bash
# Enable verbose logging
export RUST_LOG=ty_find=debug

# Run command
ty-find hover file.py --line 10 --column 5

# Logs show:
# [DEBUG] Checking for daemon at /tmp/ty-find-1000.sock
# [DEBUG] Connected to daemon
# [DEBUG] Sending request: hover
# [DEBUG] Got response in 87ms (cache miss)
```

## Future Enhancements

1. **Smart caching**: Cache invalidation based on file changes
2. **Workspace watching**: Monitor file changes and update LSP
3. **Preemptive loading**: Predict user's next query
4. **Multi-workspace optimization**: Share common type information
5. **Remote daemon**: Support remote development over TCP (with TLS)

## Conclusion

This daemon architecture provides:
- ✅ Transparent operation (auto-start)
- ✅ Fast response times (<100ms warm)
- ✅ Efficient resource usage (connection pooling)
- ✅ Graceful degradation (falls back to direct execution)
- ✅ Simple implementation (Unix sockets, JSON-RPC)

The design prioritizes simplicity and reliability over complexity, making it easy to implement and maintain while delivering significant performance improvements for CLI users.
