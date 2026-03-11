# ty-find

An **LSP adapter for AI coding agents**. Symbol name in, structured code intelligence out.

LSP servers are the gold standard for code navigation — but they require file positions (`file.py:29:7`). LLMs think in symbol names (`MyClass`). To use an LSP, an LLM first has to grep for the position, which is imprecise and adds a round-trip. **tyf bridges this gap:** one command gives you definition, signature, and references — by name, no file paths needed.

```bash
$ tyf show MyClass
MyClass
  Definition: src/models.py:15:1
  Signature:  type[MyClass]

$ tyf show MyClass --all    # add docstring + refs + test refs
```

**Built for:** Claude Code, Codex, Cursor, Gemini CLI — and humans who want fast terminal-based navigation.

## Why tyf?

**vs grep/ripgrep:**
- grep matches text — tyf understands Python's type system
- grep returns hits in comments, strings, and docstrings; tyf returns only real symbol references

**vs raw LSP (in editors):**
- LSP requires `file:line:col` positions to answer queries
- An LLM doesn't know positions without searching first
- Searching with grep is imprecise — circular problem
- tyf accepts symbol names directly, resolves positions internally

## Usage with Claude Code

Add this to your project's `CLAUDE.md` to enable type-aware code navigation:

<!-- BEGIN SHARED:claude-snippet -->
```markdown
### Python Symbol Navigation — `tyf`

This project has `tyf` — a type-aware code search that gives LSP-quality
results by symbol name. Use `tyf` instead of grep/ripgrep for Python symbol lookups.

| Task | Command |
|------|---------|
| Definition + signature | `tyf show my_function` |
| ...with docstring | `tyf show my_function --doc` |
| ...with all details | `tyf show my_function --all` |
| Find definition | `tyf find MyClass` |
| All usages (before refactoring) | `tyf refs my_function` |
| Class public API | `tyf members TheirClass` |
| File outline | `tyf list file.py` |

All commands accept multiple symbols — batch to save tool calls.
Run `tyf <cmd> --help` for options.

Use grep for: string literals, config values, TODOs, non-Python files.
```
<!-- END SHARED:claude-snippet -->

## Installation

**Prerequisite:** [ty](https://github.com/astral-sh/ty) type checker (`uv add --dev ty`)

**Optional:** [ripgrep](https://github.com/BurntSushi/ripgrep) (`rg`) — speeds up lookups for non-existent symbols by quickly verifying whether a symbol appears in any `.py` file before retrying LSP queries. Without it, searches for non-existent symbols still work but may be slower.

```bash
# macOS
brew install ripgrep

# Ubuntu/Debian
sudo apt install ripgrep

# Or via cargo
cargo install ripgrep
```

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
| `show` | Yes | No |
| `refs` | Yes | No |
| `list` | Yes | No |
| `daemon` | Yes | No |

On Windows, daemon-dependent commands exit with a clear error message. Adding the package as a dependency won't break your project on Windows — it just won't have full functionality. PRs for Windows named-pipe support are welcome!

## Usage

### Show (Definition + Signature + References)

All-in-one command — searches the workspace by symbol name, no file needed. Supports multiple symbols in a single call:

```bash
tyf show calculate_sum

# Show multiple symbols at once (results grouped by symbol)
tyf show calculate_sum UserService Config

# Include docstring
tyf show calculate_sum --doc

# Show everything (doc + refs + test refs)
tyf show calculate_sum --all

# Narrow to a specific file
tyf show calculate_sum --file src/math.py

# JSON output for scripting
tyf --format json show UserService
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
tyf --format json show MyClass
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
