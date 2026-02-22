# references

Find all references to a symbol (by position or by name)

## Usage

```
# Position mode (exact)
ty-find references -f <FILE> -l <LINE> -c <COLUMN>

# Symbol mode (parallel search)
ty-find references <SYMBOLS>... [-f <FILE>]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<SYMBOLS>...` | Symbol name(s) to find references for (symbol mode) |

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | File path (required for position mode, optional for symbol mode) |
| `-l, --line` | Line number (position mode, requires --file and --column) |
| `-c, --column` | Column number (position mode, requires --file and --line) |
| `--include-declaration` | Include the declaration in the results |

## Examples

```bash
# Position mode: exact location (pipeable from document-symbols)
ty-find references -f main.py -l 10 -c 5

# Symbol mode: find references by name
ty-find references my_function

# Symbol mode: multiple symbols searched in parallel
ty-find references my_function MyClass calculate_sum

# Symbol mode: narrow the search to a specific file
ty-find references my_function -f main.py
```

## See also

- [Commands Overview](overview.md)
