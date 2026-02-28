# What is ty-find?

ty-find is a command-line tool (`tyf`) for type-aware Python code navigation. It talks to [ty](https://github.com/astral-sh/ty)'s LSP server to find definitions, references, and type information for Python symbols — directly from the terminal. A background daemon keeps the LSP server warm so that repeated queries respond in 50–100ms.

## Why not grep?

Grep matches text. tyf understands Python's type system.

When you grep for `calculate_sum`, you'll get hits in comments, docstrings, string literals, and variable names that happen to contain the substring. tyf returns only the actual symbol definition, its type signature, and where it's referenced — because it uses ty's type inference engine under the hood.

This matters most for AI coding agents (Claude Code, Codex, etc.) that need precise symbol information to make correct edits.

## Installation

ty-find requires [ty](https://github.com/astral-sh/ty) to be installed and on PATH.

```bash
# Install ty (required)
uv add --dev ty

# Install ty-find
uv add --dev ty-find
```

Or with pip:

```bash
pip install ty-find
```

## Quick start

```bash
# Get a full overview of a symbol: definition, type, and references
$ tyf inspect MyClass

MyClass
  Definition: src/models.py:15:1
  Type: type[MyClass]
  References:
    src/main.py:3:1
    src/main.py:45:12
    tests/test_models.py:8:5

# Find where a function is defined
$ tyf find calculate_sum

calculate_sum
  src/utils.py:22:1

# See all commands
$ tyf --help
```

## For AI agents

Machine-readable documentation is available at [`llms.txt`](llms.txt) and [`llms-full.txt`](llms-full.txt), following the [llms.txt](https://llmstxt.org) convention.
