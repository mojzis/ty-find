# ty-find

A command-line tool for Python code navigation using ty's LSP server. Uses a daemon-backed architecture to keep LSP connections warm between commands (~50-100ms after initial startup).

## Usage with Claude Code

Add this to your project's `CLAUDE.md` to enable type-aware code navigation:

```markdown
### Python Symbol Navigation (ty-find)

IMPORTANT: Use `tyf` instead of Grep for Python symbol lookups.
Grep matches in comments, strings, and docs — tyf is type-aware and precise.
Run `tyf --help` to see all commands. Run `tyf <cmd> --help` for details.

- Symbol overview (definition + type + refs): `tyf inspect SymbolName`
- Find definition: `tyf find SymbolName`
- All usages before refactoring: `tyf refs SymbolName` or `tyf refs -f file.py -l LINE -c COL`
- Type info: `tyf type file.py -l LINE -c COL`
- File outline: `tyf list file.py`

Grep is still appropriate for string literals, config values, TODOs, and non-symbol text.
```

### Why ty-find over grep?

- **Find symbol usages** - grep matches in docs, comments, and strings; tyf returns only actual code references
- **Rename refactoring** - grep may miss or over-match; tyf is type-aware and precise

## Installation

**Prerequisite:** [ty](https://github.com/astral-sh/ty) type checker (`uv add --dev ty`)

### From PyPI

```bash
pip install ty-find

# Or with uv
uv add --dev ty-find
```

### From Git (Pre-Release)

Requires the Rust toolchain to build from source:

```bash
pip install "ty-find @ git+https://github.com/mojzis/ty-find.git"

# Or with uv
uv add --dev "ty-find @ git+https://github.com/mojzis/ty-find.git"
```

### From Source

```bash
git clone https://github.com/mojzis/ty-find.git
cd ty-find
cargo install --path .
```

**Note:** Windows support is limited — see [Platform Support](#platform-support) below.

## Platform Support

ty-find builds and installs on all platforms, but the background daemon requires Unix domain sockets and is only available on Unix systems (Linux, macOS).

| Command | Linux / macOS | Windows |
|---------|:---:|:---:|
| `definition` | Yes | Yes |
| `find --file` | Yes | Yes |
| `interactive` | Yes | Yes |
| `find` (no file) | Yes | No |
| `inspect` | Yes | No |
| `type` | Yes | No |
| `refs` | Yes | No |
| `workspace-symbols` | Yes | No |
| `list` | Yes | No |
| `daemon` | Yes | No |

On Windows, daemon-dependent commands exit with a clear error message. Adding the package as a dependency won't break your project on Windows — it just won't have full functionality. PRs for Windows named-pipe support are welcome!

## Usage

### Inspect (Definition + Type Info + References)

All-in-one command — searches the workspace by symbol name, no file needed. Supports multiple symbols in a single call:

```bash
tyf inspect calculate_sum

# Inspect multiple symbols at once (results grouped by symbol)
tyf inspect calculate_sum UserService Config

# Narrow to a specific file
tyf inspect calculate_sum --file src/math.py

# JSON output for scripting
tyf --format json inspect UserService
```

### Find Symbol by Name

Searches the workspace for a symbol's definition. Supports multiple symbols in a single call:

```bash
tyf find calculate_sum

# Find multiple symbols at once (results grouped by symbol)
tyf find calculate_sum multiply divide

# Narrow to a specific file (text-based search + goto_definition)
tyf find function_name --file myfile.py
```

### Type (Type Information)

```bash
tyf type src/main.py --line 45 --column 12

# JSON output for scripting
tyf --format json type src/main.py -l 45 -c 12 | jq '.result.contents.value'
```

### Go to Definition

```bash
tyf definition myfile.py --line 10 --column 5
```

### Find References

```bash
# By position (exact, pipeable from list)
tyf refs -f myfile.py --line 10 --column 5

# By name (parallel search)
tyf refs my_function MyClass
```

### Workspace Symbol Search

```bash
tyf workspace-symbols --query "UserService"
```

### Document Outline

```bash
tyf list src/services/user.py
```

### Interactive Mode

```bash
tyf interactive
```

### Daemon Management

The daemon starts automatically on first use. Manual control:

```bash
tyf daemon start    # Start manually
tyf daemon status   # Check status
tyf daemon stop     # Stop
```

## Output Formats

All commands support `--format` (placed before the subcommand): `human` (default), `json`, `csv`, `paths`.

```bash
tyf --format json type myfile.py -l 10 -c 5
tyf --format csv workspace-symbols --query "User"
```

## Architecture

```
CLI Command → Daemon Client (auto-connects) → Unix Socket
→ Daemon Server (5min idle timeout) → LSP Client Pool → ty LSP Server
```

The daemon keeps LSP connections warm: first command takes 1-2s, subsequent commands 50-100ms.

## Development

```bash
cargo build --release
cargo test
cargo clippy
cargo fmt --check

# Verbose logging
RUST_LOG=ty_find=debug cargo run -- type test.py -l 1 -c 1
```

## Troubleshooting

```bash
# Check ty is installed
ty --version

# Debug daemon issues
tyf daemon status
RUST_LOG=ty_find=debug tyf daemon start

# Restart daemon
tyf daemon stop && tyf daemon start
```

## Contributing

Contributions welcome! Please open an issue to discuss major changes.

## License

MIT License - see LICENSE file for details.

## Credits

Built with [ty](https://github.com/astral-sh/ty) - Astral's Python type checker.
