# ty-find: CLI-First Approach for Claude Code Integration

**Date**: 2025-11-17
**Philosophy**: Fast CLI tools > Yet Another MCP Server

## The Better Approach

Instead of creating an MCP server, make ty-find a **blazingly fast CLI tool** that Claude Code (and any other tool) can call directly.

## Why CLI > MCP

### Problems with MCP Approach
- ❌ MCP fatigue - users don't want another server to manage
- ❌ Configuration complexity
- ❌ Process management overhead
- ❌ Limited to MCP-aware tools
- ❌ Harder to test and debug

### Benefits of CLI Approach
- ✅ Universal - works with any tool that can call CLI
- ✅ Simple - just execute a command
- ✅ Easy to test - `ty-find hover file.py --line 10 --col 5 --format json`
- ✅ Easy to debug - can run manually
- ✅ No daemon management needed (from user perspective)
- ✅ Composable with other CLI tools
- ✅ Works in CI/CD, scripts, makefiles, etc.

## Architecture

```
┌──────────────┐
│ Claude Code  │
└──────┬───────┘
       │ Bash tool (direct CLI calls)
       │
       │ $ ty-find hover file.py --line 10 --col 5 --format json
       │
┌──────▼───────┐
│   ty-find    │ (fast CLI with daemon backend)
│   CLI        │
└──────┬───────┘
       │ (auto-connects to daemon if running)
       │
┌──────▼───────┐
│  ty-find     │ (background daemon)
│  daemon      │
└──────┬───────┘
       │
┌──────▼───────┐
│   ty lsp     │
└──────────────┘
```

## Claude Code Usage Examples

### Example 1: Get Type Information

```bash
# Claude Code calls:
ty-find hover src/main.py --line 45 --column 12 --format json

# Returns instantly (daemon keeps LSP warm):
{
  "symbol": "UserService",
  "type": "class UserService(BaseService[User])",
  "documentation": "Service for managing user accounts...",
  "signature": "__init__(self, db: Database, cache: Cache)"
}
```

### Example 2: Find Symbol Definition

```bash
ty-find definition src/api/routes.py --line 23 --column 8 --format json

# Returns:
{
  "query": "src/api/routes.py:23:8",
  "results": [
    {
      "file": "src/services/user.py",
      "line": 15,
      "column": 6,
      "symbol": "create_user",
      "context": "def create_user(self, name: str, email: str) -> User:"
    }
  ]
}
```

### Example 3: Search Workspace

```bash
ty-find workspace-symbols --query "UserService" --format json

# Returns:
{
  "results": [
    {
      "name": "UserService",
      "kind": "class",
      "file": "src/services/user.py",
      "line": 10,
      "column": 6
    },
    {
      "name": "UserServiceTest",
      "kind": "class",
      "file": "tests/test_user_service.py",
      "line": 5,
      "column": 6
    }
  ]
}
```

### Example 4: Get Diagnostics

```bash
ty-find diagnostics src/api/ --format json

# Returns:
{
  "errors": [
    {
      "file": "src/api/routes.py",
      "line": 45,
      "column": 12,
      "severity": "error",
      "message": "Argument of type 'str | None' cannot be assigned to parameter 'name' of type 'str'",
      "code": "reportArgumentType"
    }
  ],
  "warnings": [...],
  "count": {"errors": 3, "warnings": 7}
}
```

## Making It Fast: Daemon Architecture

**Key insight**: Daemon runs in background, CLI connects to it instantly.

### User Experience

```bash
# First command starts daemon automatically (one-time cost)
$ ty-find hover file.py --line 10 --col 5
# ~1-2s (starts daemon + LSP + query)

# All subsequent commands are instant
$ ty-find hover file.py --line 20 --col 8
# ~50-100ms (daemon already running)

# Daemon auto-stops after idle timeout (e.g., 5 minutes)
# Or user can manage explicitly:
$ ty-find daemon start    # Pre-start daemon
$ ty-find daemon stop     # Stop daemon
$ ty-find daemon status   # Check status
```

### Implementation Strategy

#### Phase 1: Smart Auto-Daemon

CLI tool automatically:
1. Checks if daemon is running (Unix socket / named pipe)
2. If not running: spawn daemon in background
3. Connects to daemon and sends request
4. Returns result

```rust
// In main.rs
async fn execute_command(cmd: Commands) -> Result<()> {
    // Try to connect to existing daemon
    if let Ok(client) = DaemonClient::connect().await {
        return client.execute(cmd).await;
    }

    // No daemon? Start one in background
    DaemonServer::spawn_background()?;

    // Wait a bit for daemon to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now connect and execute
    let client = DaemonClient::connect().await?;
    client.execute(cmd).await
}
```

#### Phase 2: Connection Pooling

Daemon maintains:
- LSP client pool (one per workspace)
- Response cache (for repeated queries)
- Smart preloading (predict next queries)

```rust
struct DaemonServer {
    // Key: workspace path
    // Value: TyLspClient
    lsp_clients: HashMap<PathBuf, Arc<TyLspClient>>,

    // Cache recent queries
    cache: LruCache<QueryKey, QueryResult>,

    // Auto-cleanup after idle time
    last_activity: Instant,
}
```

### Communication Protocol

Use Unix domain sockets (Linux/Mac) or named pipes (Windows):

```
Client → Daemon: JSON-RPC request
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "hover",
  "params": {
    "file": "/path/to/file.py",
    "line": 10,
    "column": 5,
    "workspace": "/path/to/workspace"
  }
}

Daemon → Client: JSON-RPC response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "symbol": "foo",
    "type": "str",
    ...
  }
}
```

## Performance Targets

### Without Daemon (Current)
- Cold start: 1-2s (spawn ty lsp + initialize + query)
- Each command: 1-2s (no caching)

### With Daemon (Goal)
- First command: 1-2s (start daemon, one-time)
- Subsequent: 50-100ms (warm cache)
- Cache hits: 5-10ms (instant)

### Comparison
```
# 10 commands without daemon: ~15-20 seconds
# 10 commands with daemon: ~2s + (10 * 0.1s) = ~3 seconds
# 5-6x speedup!
```

## JSON Output Schema

Design clean, consistent JSON output for all commands:

### Success Response
```json
{
  "status": "success",
  "query": {
    "command": "hover",
    "file": "src/main.py",
    "line": 45,
    "column": 12
  },
  "result": {
    // Command-specific data
  },
  "timing": {
    "query_ms": 87,
    "cache_hit": false
  }
}
```

### Error Response
```json
{
  "status": "error",
  "error": {
    "code": "FILE_NOT_FOUND",
    "message": "File not found: src/main.py",
    "suggestion": "Check that the file path is correct"
  }
}
```

## Claude Code Integration Pattern

Claude Code can use ty-find like any CLI tool:

```python
# Claude Code's internal logic (conceptual)

def understand_symbol(file_path, line, column):
    """Get type information for a symbol"""
    result = subprocess.run(
        ["ty-find", "hover", file_path,
         "--line", str(line), "--column", str(column),
         "--format", "json"],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)

def find_definition(file_path, line, column):
    """Find where a symbol is defined"""
    result = subprocess.run(
        ["ty-find", "definition", file_path,
         "--line", str(line), "--column", str(column),
         "--format", "json"],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)
```

### Smart Usage Patterns

**Pattern 1: Understand before modifying**
```
User: "Update the create_user function to add email validation"

Claude Code:
1. Grep for "create_user" → finds src/services/user.py:45
2. $ ty-find hover src/services/user.py --line 45 --column 8 --format json
   → Gets full signature and types
3. $ ty-find definition src/services/user.py --line 45 --column 8 --format json
   → Confirms exact location
4. Makes type-aware changes
```

**Pattern 2: Find all usages**
```
User: "Change the UserService constructor"

Claude Code:
1. Find UserService definition
2. $ ty-find find-references src/services/user.py --line 15 --column 6 --format json
   → Gets all instantiation sites
3. Update all locations consistently
```

**Pattern 3: Workspace-wide search**
```
User: "Where do we handle authentication?"

Claude Code:
1. $ ty-find workspace-symbols --query "auth" --format json
   → Gets all auth-related symbols
2. $ ty-find hover <each result> --format json
   → Gets type info for each
3. Provides comprehensive answer
```

## Revised Roadmap

### Phase 1: Fix Build & Core Features (Week 1-2)
- [ ] Fix tracing-subscriber dependency
- [ ] Verify all existing commands work
- [ ] Ensure JSON output is clean and parseable
- [ ] Add comprehensive tests

### Phase 2: Expand LSP Features (Week 3-4)
- [ ] Add `hover` command
- [ ] Add `workspace-symbols` command
- [ ] Add `document-symbols` command
- [ ] Add `diagnostics` command
- [ ] Consistent JSON output for all commands

### Phase 3: Daemon Mode (Week 5-6) 🔥 **NOW CRITICAL**
- [ ] Design daemon protocol (Unix socket/named pipe)
- [ ] Implement daemon server
- [ ] Auto-start daemon from CLI
- [ ] LSP connection pooling
- [ ] Response caching
- [ ] Benchmark performance improvements

### Phase 4: Production Ready (Week 7-8)
- [ ] Comprehensive error handling
- [ ] Clean error messages in JSON
- [ ] Installation via pip works perfectly
- [ ] CI/CD for releases
- [ ] Documentation with JSON examples
- [ ] Performance profiling

### Phase 5: Advanced Features (Week 9+)
- [ ] Find references (when available in ty)
- [ ] Rename support (when available)
- [ ] Smart caching strategies
- [ ] Workspace indexing optimization
- [ ] Claude Code usage guide

## Documentation for Claude Code Users

### Quick Start

```bash
# Install
pip install ty-find

# Use with Claude Code (automatically)
# Claude Code will call these commands when analyzing Python code

# Manual usage
ty-find hover myfile.py --line 10 --column 5 --format json
ty-find definition myfile.py --line 10 --column 5 --format json
ty-find workspace-symbols --query "MyClass" --format json
```

### Configuration

```toml
# In .claude/config.toml or pyproject.toml
[tool.ty-find]
# Daemon settings
daemon_auto_start = true
daemon_idle_timeout = 300  # seconds

# Performance
cache_size = 1000
preload_workspace = true

# Output
default_format = "json"
```

## Key Advantages Over MCP

1. **Simplicity**: Just a CLI tool, no server management
2. **Speed**: Daemon in background, but hidden from user
3. **Universality**: Works with Claude Code, Cursor, shell scripts, CI/CD
4. **Debuggability**: Can run commands manually to test
5. **Composability**: Can pipe to jq, combine with other tools
6. **Reliability**: CLI crashes don't affect daemon, daemon crashes don't affect CLI

## Comparison

```
MCP Approach:
User → Claude Code → MCP Protocol → ty-find MCP Server → ty LSP
      (complex)     (protocol overhead) (another server)

CLI Approach:
User → Claude Code → Bash → ty-find CLI → ty-find daemon → ty LSP
      (simple)      (direct) (hidden)     (fast)
```

## Example: Real-World Claude Code Session

```
User: "Refactor the authentication system to use async/await"

Claude Code thinking:
1. Find auth code:
   $ ty-find workspace-symbols --query "auth" --format json

2. Understand current implementation:
   $ ty-find hover src/auth.py --line 15 --column 6 --format json
   → "def authenticate(username: str, password: str) -> bool:"

3. Find all usages:
   $ ty-find find-references src/auth.py --line 15 --column 6 --format json
   → 15 locations across 6 files

4. Check for issues before refactoring:
   $ ty-find diagnostics src/auth.py --format json

5. Make changes with type-aware understanding
6. Verify no new type errors:
   $ ty-find diagnostics src/ --format json

Total time with daemon: ~1-2 seconds
Total time without daemon: ~10-15 seconds
```

## Conclusion

**CLI-first approach is superior because:**

1. ✅ No MCP fatigue - just a fast CLI tool
2. ✅ Works everywhere - not just Claude Code
3. ✅ Simpler architecture - daemon is hidden implementation detail
4. ✅ Better DX - can test commands manually
5. ✅ More maintainable - standard CLI patterns

**Daemon mode becomes CRITICAL** (not optional) because:
- Without it, each command takes 1-2s (unacceptable for AI tools)
- With it, each command takes 50-100ms (acceptable)
- User never needs to think about daemon - it's automatic

**Next Steps:**
1. Fix build issues
2. Add more LSP features as CLI commands
3. Implement auto-daemon architecture ← **MOST IMPORTANT**
4. Optimize JSON output for programmatic consumption
5. Document usage patterns for Claude Code

The beauty is: it's just a fast CLI tool that Claude Code calls via Bash. Simple, universal, and fast.
