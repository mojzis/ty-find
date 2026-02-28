# refs

All usages of a symbol across the codebase. Useful before renaming or removing code to understand the impact.

## Usage

```
# Position mode (exact)
tyf refs -f <FILE> -l <LINE> -c <COLUMN>

# Symbol mode (parallel search)
tyf refs <QUERIES>... [-f <FILE>]

# Stdin mode (pipe positions or symbol names)
... | tyf refs --stdin
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
tyf refs -f main.py -l 10 -c 5

# Symbol mode: find references by name
tyf refs my_function

# Symbol mode: multiple symbols searched in parallel
tyf refs my_function MyClass calculate_sum

# Auto-detected file:line:col positions (parallel)
tyf refs main.py:10:5 utils.py:20:3

# Mixed: positions and symbols together
tyf refs main.py:10:5 my_function

# Pipe from list
tyf list file.py --format csv \
  | awk -F, 'NR>1{printf "file.py:%s:%s\n",$3,$4}' \
  | tyf refs --stdin

# Pipe symbol names
tyf list file.py --format csv \
  | tail -n+2 | cut -d, -f1 \
  | tyf refs --stdin
```

## See also

- [Commands Overview](overview.md)
