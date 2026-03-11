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
