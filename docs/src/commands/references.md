# references

Find every place a symbol is used across the codebase. Useful before renaming or removing code to understand the impact.

## Usage

```
# Position mode (exact)
ty-find references -f <FILE> -l <LINE> -c <COLUMN>

# Symbol mode (parallel search)
ty-find references <QUERIES>... [-f <FILE>]

# Stdin mode (pipe positions or symbol names)
... | ty-find references --stdin
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<QUERIES>...` | Symbol names or `file:line:col` positions (auto-detected) |

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | File path (required for position mode, optional for symbol mode) |
| `-l, --line` | Line number (position mode, requires --file and --column) |
| `-c, --column` | Column number (position mode, requires --file and --line) |
| `--stdin` | Read queries from stdin (one per line) |
| `--include-declaration` | Include the declaration in the results |

## Examples

```bash
# Position mode: exact location
ty-find references -f main.py -l 10 -c 5

# Symbol mode: find references by name
ty-find references my_function

# Symbol mode: multiple symbols searched in parallel
ty-find references my_function MyClass calculate_sum

# Auto-detected file:line:col positions (parallel)
ty-find references main.py:10:5 utils.py:20:3

# Mixed: positions and symbols together
ty-find references main.py:10:5 my_function

# Pipe from document-symbols
ty-find document-symbols file.py --format csv \
  | awk -F, 'NR>1{printf "file.py:%s:%s\n",$3,$4}' \
  | ty-find references --stdin

# Pipe symbol names
ty-find document-symbols file.py --format csv \
  | tail -n+2 | cut -d, -f1 \
  | ty-find references --stdin
```

## See also

- [Commands Overview](overview.md)
