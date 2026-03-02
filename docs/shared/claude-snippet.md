### Python Symbol Navigation (ty-find)

IMPORTANT: Use `tyf` instead of Grep for Python symbol lookups.
Grep matches in comments, strings, and docs — tyf is type-aware and precise.
Run `tyf --help` to see all commands. Run `tyf <cmd> --help` for details.

- Symbol overview (definition + type + refs): `tyf inspect my_function`
- Find definition: `tyf find MyClass`
- Class public interface: `tyf members TheirClass`
- All usages before refactoring: `tyf refs my_function` or `tyf refs -f file.py -l LINE -c COL`
- File outline: `tyf list file.py`

All commands accept multiple symbols in one call — batch to save tool invocations.

Grep is still appropriate for string literals, config values, TODOs, and non-symbol text.
