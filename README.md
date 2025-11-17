# ty-find

A blazingly fast command-line tool for Python code navigation using ty's LSP server. Features daemon-backed architecture for sub-100ms response times.

## Features

- 🚀 **Fast**: Daemon mode keeps LSP warm (50-100ms vs 1-2s per command)
- 🔍 **Type-aware**: Uses ty's LSP for accurate symbol resolution
- 📦 **Multiple commands**: hover, definition, symbols, outline
- 🎯 **JSON output**: Perfect for AI coding tools like Claude Code
- 🔄 **Auto-daemon**: Transparently starts background daemon on first use
- 🛡️ **User-isolated**: Each user gets their own daemon process

## Installation

### Prerequisites
- [ty](https://github.com/astral-sh/ty) type checker: `pip install ty`

### Quick Install (Recommended)

Pre-built wheels are available for Linux and macOS:

```bash
# Install from PyPI (coming soon - once first release is published)
pip install ty-find

# Or with uv
uv pip install ty-find
```

**Note:** Windows is not currently supported. PRs welcome!

### For Python Projects

Add to your `pyproject.toml`:

```toml
# For pip/setuptools projects
[project.optional-dependencies]
dev = [
    "ty-find",  # Once published to PyPI
]

# For uv projects (recommended)
[dependency-groups]
dev = [
    "ty-find",
]
```

### Install from Git (Pre-Release)

Until the first PyPI release, install from Git:

```bash
# Requires Rust toolchain to build from source
pip install "ty-find @ git+https://github.com/mojzis/ty-find.git"

# Or with uv
uv pip install "ty-find @ git+https://github.com/mojzis/ty-find.git"
```

**Note:** Installing from Git requires the Rust toolchain. Pre-built wheels eliminate this requirement.

### From Source (Development)

```bash
git clone https://github.com/mojzis/ty-find.git
cd ty-find
cargo install --path .
```

## Usage

### Type Information (Hover)

Get type information and documentation at a specific position:

```bash
ty-find hover src/main.py --line 45 --column 12

# Output:
Type: UserService
Documentation: Service for managing user accounts
Signature: class UserService(BaseService[User])

# JSON output for programmatic use
ty-find hover src/main.py --line 45 --column 12 --format json
```

### Go to Definition

Find where a symbol is defined:

```bash
ty-find definition myfile.py --line 10 --column 5

# Output:
Definition: src/services/user.py:15:6
def create_user(name: str, email: str) -> User:
```

### Search Symbols Across Workspace

Search for symbols across your entire codebase:

```bash
ty-find workspace-symbols --query "UserService"

# Output:
UserService (class) - src/services/user.py:10:6
UserServiceTest (class) - tests/test_user_service.py:5:6

# JSON output
ty-find workspace-symbols --query "auth" --format json
```

### Document Outline

Get the structure/outline of a file:

```bash
ty-find document-symbols src/services/user.py

# Output:
UserService (class)
  ├─ __init__ (method)
  ├─ create_user (method)
  ├─ get_user (method)
  └─ update_user (method)
```

### Find Symbol by Name

Find all occurrences of a symbol in a file:

```bash
ty-find find myfile.py function_name
```

### Interactive Mode

REPL-style interface for exploring code:

```bash
ty-find interactive
> myfile.py:10:5
> find myfile.py function_name
> quit
```

### Daemon Management

The daemon starts automatically, but you can manage it manually:

```bash
# Start daemon (optional - auto-starts on first use)
ty-find daemon start

# Check daemon status
ty-find daemon status
# Output:
# Daemon: running
# Uptime: 5m 23s
# Active workspaces: 2

# Stop daemon
ty-find daemon stop
```

## Output Formats

All commands support multiple output formats:

```bash
# Human-readable (default)
ty-find hover myfile.py -l 10 -c 5

# JSON (for Claude Code, scripts, etc.)
ty-find hover myfile.py -l 10 -c 5 --format json

# CSV
ty-find workspace-symbols --query "User" --format csv

# Paths only
ty-find definition myfile.py -l 10 -c 5 --format paths
```

## Performance

### Without Daemon (Old Approach)
- Each command: **1-2 seconds**
- 10 commands: **~15-20 seconds**

### With Daemon (Current)
- First command: **1-2 seconds** (starts daemon + LSP)
- Subsequent: **50-100ms** (warm cache)
- 10 commands: **~3 seconds**
- **5-6x faster!** 🚀

## Use with Claude Code

ty-find works perfectly with Claude Code as a fast CLI tool:

```python
# Claude Code can call ty-find directly via Bash

# Get type information
result = subprocess.run([
    "ty-find", "hover", "src/main.py",
    "--line", "45", "--column", "12",
    "--format", "json"
], capture_output=True, text=True)

# Search for symbols
result = subprocess.run([
    "ty-find", "workspace-symbols",
    "--query", "UserService",
    "--format", "json"
], capture_output=True, text=True)
```

This provides Claude Code with:
- ✅ Type-aware understanding of Python code
- ✅ Accurate symbol resolution (not just grep)
- ✅ Fast responses (<100ms) for interactive use
- ✅ 30-50% token reduction vs reading entire files

## Architecture

```
CLI Command
    ↓
Daemon Client (auto-connects)
    ↓
Unix Socket (/tmp/ty-find-{uid}.sock)
    ↓
Daemon Server (auto-started, 5min idle timeout)
    ↓
LSP Client Pool (one per workspace)
    ↓
ty LSP Server (spawned on-demand)
```

The daemon runs in the background and keeps LSP connections warm, providing fast responses for all subsequent commands.

## Examples

### Basic Usage

```bash
# Get type at cursor position
ty-find hover src/calculator.py --line 25 --column 10

# Find where 'calculate_total' is defined
ty-find find src/calculator.py calculate_total

# Search for all authentication-related symbols
ty-find workspace-symbols --query "auth"

# Get file structure
ty-find document-symbols src/api/routes.py
```

### With Workspace Specification

```bash
ty-find hover src/app.py -l 15 -c 8 --workspace /path/to/project
```

### JSON Output for Scripting

```bash
# Get symbol information as JSON
ty-find hover src/main.py -l 45 -c 12 --format json | jq '.result.contents.value'

# Find all class definitions
ty-find workspace-symbols --query "" --format json | jq '.results[] | select(.kind == 5)'
```

## Available Commands

| Command | Description | Example |
|---------|-------------|---------|
| `hover` | Get type information at position | `ty-find hover file.py -l 10 -c 5` |
| `definition` | Go to definition | `ty-find definition file.py -l 10 -c 5` |
| `workspace-symbols` | Search symbols across workspace | `ty-find workspace-symbols --query "User"` |
| `document-symbols` | Get file outline | `ty-find document-symbols file.py` |
| `find` | Find symbol by name in file | `ty-find find file.py symbol_name` |
| `interactive` | Interactive REPL mode | `ty-find interactive` |
| `daemon start` | Start daemon manually | `ty-find daemon start` |
| `daemon stop` | Stop daemon | `ty-find daemon stop` |
| `daemon status` | Check daemon status | `ty-find daemon status` |

## Documentation

- [Implementation Summary](docs/implementation-summary.md) - What was implemented and how
- [Daemon Architecture](docs/daemon-architecture.md) - Technical details of daemon design
- [CLI-First Approach](plans/cli-first-approach.md) - Why CLI over MCP
- [Project Roadmap](plans/project-assessment-and-roadmap.md) - Future plans

## Troubleshooting

### Daemon won't start
```bash
# Check if ty is installed
ty --version

# Check socket path
ls -la /tmp/ty-find-*.sock

# Try starting daemon manually with verbose logging
RUST_LOG=ty_find=debug ty-find daemon start
```

### Slow responses
```bash
# Check daemon status
ty-find daemon status

# Restart daemon
ty-find daemon stop
ty-find daemon start
```

### ty not found
```bash
# Install ty
pip install ty

# Verify installation
ty --version
```

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run with verbose logging
RUST_LOG=ty_find=debug cargo run -- hover test.py -l 1 -c 1

# Check code
cargo clippy
cargo fmt --check
```

## Contributing

Contributions welcome! Please:
1. Open an issue to discuss major changes
2. Follow existing code style
3. Add tests for new features
4. Update documentation

## License

MIT License - see LICENSE file for details

## Credits

- Built with [ty](https://github.com/astral-sh/ty) - Astral's blazingly fast Python type checker
- Inspired by the need for fast, type-aware Python code navigation in AI coding tools
