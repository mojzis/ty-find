# What is ty-find?

ty-find (`tyf`) is an LSP adapter that lets AI coding agents query Python's type system by symbol name. It wraps [ty](https://github.com/astral-sh/ty)'s LSP server so that `tyf show MyClass` returns the definition location, type signature, and all references — without requiring file paths or line numbers. A background daemon keeps responses under 100ms.

Built for Claude Code, Codex, Cursor, Gemini CLI — and humans who want fast Python navigation from the terminal.

## Why tyf?

### vs grep/ripgrep

grep matches text. tyf understands Python's type system.

When you grep for `calculate_sum`, you get hits in comments, docstrings, string literals, and variable names that happen to contain the substring. tyf returns only the actual symbol definition, its type signature, and where it's referenced — because it uses ty's type inference engine under the hood.

### vs raw LSP (in editors)

LSP servers are the gold standard for code intelligence, but they require file positions (`file.py:29:7`) to answer queries. An LLM doesn't know positions — it thinks in symbol names (`MyClass`). To use an LSP, it would first need to grep for the position, which is imprecise and adds a round-trip.

tyf breaks this cycle: symbol name in → structured LSP knowledge out. No file paths, no line numbers, no grep step needed.

## Installation

ty-find requires [ty](https://github.com/astral-sh/ty) to be installed and on PATH.

```bash
# Install ty (required)
uv add --dev ty

# Install ty-find
uv add --dev ty-find
```

## Quick start

```
# Definition + signature (default)
$ tyf show list_animals
# Definition (func)
main.py:14:1

# Signature
def list_animals(animals: list[Animal]) -> None

# Refs: 2 across 1 file(s)

# Everything: docs + refs + test refs
$ tyf show list_animals --all

# See all commands
$ tyf --help
```

## For AI agents

Machine-readable documentation is available at [`llms.txt`](llms.txt) and [`llms-full.txt`](llms-full.txt), following the [llms.txt](https://llmstxt.org) convention.
