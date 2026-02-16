# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ty-find is a command-line tool that interfaces with ty's LSP server to provide go-to-definition functionality for Python functions, classes, and variables from the terminal. It's a hybrid Rust/Python project that builds a Rust binary but packages it as a Python package using maturin for easy distribution via pip/uv.

## Common Commands

### Build and Development
```bash
# Build Rust binary for development
cargo build

# Build optimized release version
cargo build --release

# Run the tool directly during development
cargo run -- definition test_example.py --line 1 --column 5

# Install ty (required for integration tests)
pip install ty

# Test the Rust code
cargo test

# Run integration tests (requires ty on PATH)
cargo test --test test_basic

# Check Python packaging works
python test_installation.py
```

### Python Packaging (maturin)
```bash
# Build Python wheel with Rust binary
maturin build

# Build and install locally for testing
maturin develop

# Build release wheel
maturin build --release
```

### Testing with ty LSP server
```bash
# Requires ty to be installed: pip install ty
ty-find definition test_example.py --line 6 --column 5
ty-find find test_example.py calculate_sum
ty-find interactive
```

## Pre-commit Checks

**Always run all checks before committing to avoid CI pipeline failures:**

```bash
cargo fmt --check && cargo clippy --all-features -- -D warnings && cargo test --all-features
```

If formatting fails, fix it with `cargo fmt` and re-run the checks.

## Architecture

### Core Components
1. **LSP Client (`src/lsp/`)** - JSON-RPC client that communicates with ty's LSP server
2. **CLI Interface (`src/cli/`)** - Command-line argument parsing and output formatting  
3. **Workspace Detection (`src/workspace/`)** - Python project detection and symbol finding
4. **Main Application (`src/main.rs`)** - Orchestrates the three main modes: definition, find, interactive

### Key Architectural Patterns

**LSP Communication Flow**:
- `TyLspServer` spawns and manages the `ty lsp` process
- `TyLspClient` handles JSON-RPC protocol with initialization, requests, and response parsing
- Communication is async using tokio with proper message framing (Content-Length headers)

**Dual Build System**:
- `Cargo.toml` defines the Rust binary with CLI dependencies (clap, tokio, serde)
- `pyproject.toml` uses maturin backend to package the Rust binary as a Python wheel
- `ty_find/__init__.py` provides Python entry point that executes the Rust binary

**Command Processing**:
- Three main commands: `definition` (find at specific line/column), `find` (search symbol), `interactive` (REPL mode)
- `SymbolFinder` does text-based symbol matching with whole-word detection
- `OutputFormatter` supports multiple formats: human, JSON, CSV, paths-only

### Python Integration Strategy
The project uses maturin to bridge Rust and Python ecosystems:
- Rust binary provides performance for LSP communication
- Python packaging allows `pip install` and `uv sync` integration
- Users add `ty-find @ git+https://github.com/user/ty-find.git` to pyproject.toml
- maturin automatically builds Rust binary during Python package installation

### Dependencies
- **ty LSP server** must be available in PATH (users install via `pip install ty`)
- **Rust toolchain** required for building from source
- **tokio** for async LSP communication and process management
- **clap** for CLI parsing with subcommands and multiple output formats