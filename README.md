# ty-find

A command-line tool for Python code navigation using ty's LSP server. Uses a daemon-backed architecture to keep LSP connections warm between commands (~50-100ms after initial startup).

## Usage with Claude Code

Add this to your project's `CLAUDE.md` to enable type-aware code navigation:

```markdown
### Code Navigation (ty-find)
Use `ty-find` for type-aware Python code navigation - more accurate than grep for symbols.

**Commands** (use relative paths from repo root):
ty-find references path/to/file.py -l LINE -c COL   # Find all usages of symbol
ty-find definition path/to/file.py -l LINE -c COL   # Go to definition
ty-find hover path/to/file.py -l LINE -c COL        # Get type info
ty-find find path/to/file.py SymbolName              # Find symbol by name in file
ty-find workspace-symbols --query "ClassName"        # Search symbols across codebase
ty-find document-symbols path/to/file.py             # Get file outline

**When to use:**
- Before renaming/refactoring: `ty-find references` to find all usages
- Understanding unfamiliar code: `ty-find hover` for type info
- Finding class/function definitions: `ty-find workspace-symbols`

**Output formats:** Add `--format json` before subcommand for programmatic use.
```

### Why ty-find over grep?

- **Find symbol usages** - grep matches in docs, comments, and strings; ty-find returns only actual code references
- **Rename refactoring** - grep may miss or over-match; ty-find is type-aware and precise

## Installation

**Prerequisite:** [ty](https://github.com/astral-sh/ty) type checker (`pip install ty`)

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

**Note:** Windows is not currently supported. PRs welcome!

## Usage

### Hover (Type Information)

```bash
ty-find hover src/main.py --line 45 --column 12

# JSON output for scripting
ty-find --format json hover src/main.py -l 45 -c 12 | jq '.result.contents.value'
```

### Go to Definition

```bash
ty-find definition myfile.py --line 10 --column 5
```

### Find References

```bash
ty-find references myfile.py --line 10 --column 5
```

### Workspace Symbol Search

```bash
ty-find workspace-symbols --query "UserService"
```

### Document Outline

```bash
ty-find document-symbols src/services/user.py
```

### Find Symbol by Name

```bash
ty-find find myfile.py function_name
```

### Interactive Mode

```bash
ty-find interactive
```

### Daemon Management

The daemon starts automatically on first use. Manual control:

```bash
ty-find daemon start    # Start manually
ty-find daemon status   # Check status
ty-find daemon stop     # Stop
```

## Output Formats

All commands support `--format` (placed before the subcommand): `human` (default), `json`, `csv`, `paths`.

```bash
ty-find --format json hover myfile.py -l 10 -c 5
ty-find --format csv workspace-symbols --query "User"
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
RUST_LOG=ty_find=debug cargo run -- hover test.py -l 1 -c 1
```

## Troubleshooting

```bash
# Check ty is installed
ty --version

# Debug daemon issues
ty-find daemon status
RUST_LOG=ty_find=debug ty-find daemon start

# Restart daemon
ty-find daemon stop && ty-find daemon start
```

## Documentation

- [Implementation Summary](docs/implementation-summary.md)
- [Daemon Architecture](docs/daemon-architecture.md)
- [CLI-First Approach](plans/cli-first-approach.md)
- [Project Roadmap](plans/project-assessment-and-roadmap.md)

## Contributing

Contributions welcome! Please open an issue to discuss major changes.

## License

MIT License - see LICENSE file for details.

## Credits

Built with [ty](https://github.com/astral-sh/ty) - Astral's Python type checker.
