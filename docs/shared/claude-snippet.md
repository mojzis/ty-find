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
