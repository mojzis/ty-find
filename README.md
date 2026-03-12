# ty-find

An **LSP adapter for AI coding agents**. Symbol name in, structured code intelligence out.

LSP servers are the gold standard for code navigation — but they require file positions (`file.py:29:7`). LLMs think in symbol names (`MyClass`). To use an LSP, an LLM first has to grep for the position, which is imprecise and adds a round-trip. **tyf bridges this gap:** one command gives you definition, signature, and references — by name, no file paths needed.

```
$ tyf show list_animals
# Definition (func)
main.py:14:1

# Signature
def list_animals(animals: list[Animal]) -> None

# Refs: 2 across 1 file(s)

$ tyf show list_animals --all    # add docs, refs, test refs
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

- `tyf show my_function` — definition + signature (add `-d` docs, `-r` refs, `-t` test refs, or `--all`)
- `tyf find MyClass` — find definition location
- `tyf refs my_function` — all usages (before refactoring)
- `tyf members TheirClass` — class public API
- `tyf list file.py` — file outline

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

```bash
uv add --dev ty-find
```

**Note:** On Windows, only `tyf find --file` is supported for now. All other commands require Unix domain sockets (Linux, macOS).

## Usage

### Show (Definition + Signature + References)

All-in-one command — searches the workspace by symbol name, no file needed. Add `-d` (docs), `-r` (references), `-t` (test refs), or `--all` for everything:

```bash
tyf show calculate_sum

# Multiple symbols at once
tyf show calculate_sum UserService Config

# Include docstring + refs + test refs
tyf show calculate_sum --all

# Narrow to a specific file
tyf show calculate_sum --file src/math.py
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

### Members (Class Public API)

```bash
tyf members MyClass
```

### Document Outline

```bash
tyf list src/services/user.py
```

### Daemon Management

The daemon starts automatically on first use. Run `tyf daemon --help` for manual control.

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

The daemon keeps LSP connections warm: first command takes 1-2s, subsequent commands 50-100ms. See [How it works](https://mojzis.github.io/ty-find/how-it-works.html) for details.

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
