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
| `find --file` | Yes | Yes |
| `find` (no file) | Yes | No |
| `find --fuzzy` | Yes | No |
| `inspect` | Yes | No |
| `refs` | Yes | No |
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

Searches the workspace for a symbol's definition. Supports multiple symbols in a single call. Use `--fuzzy` for partial/prefix matching with richer output (kind + container):

```bash
tyf find calculate_sum

# Find multiple symbols at once (results grouped by symbol)
tyf find calculate_sum multiply divide

# Narrow to a specific file (text-based search + goto_definition)
tyf find function_name --file myfile.py

# Fuzzy/prefix match (returns symbol kind + container info)
tyf find handle_ --fuzzy
```

### Find References

```bash
# By position (exact, pipeable from list)
tyf refs -f myfile.py --line 10 --column 5

# By name
tyf refs my_function MyClass

# Mixed and piped
tyf refs file.py:10:5 my_func
... | tyf refs --stdin
```

### Document Outline

```bash
tyf list src/services/user.py
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
tyf --format json inspect MyClass
tyf --format csv find User --fuzzy
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
RUST_LOG=ty_find=debug cargo run -- find hello_world
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
