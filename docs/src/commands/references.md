# references

Find all references to symbols by name (searches in parallel)

## Usage

```
ty-find references <SYMBOLS>... [OPTIONS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<SYMBOLS>...` | Symbol name(s) to find references for (supports multiple symbols) *(required)* |

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | Optional file to narrow the search (uses workspace symbols if omitted) |
| `--include-declaration` | Include the declaration in the results |

## Examples

```bash
# Find all references to a single symbol
ty-find references my_function

# Find references for multiple symbols in parallel
ty-find references my_function MyClass calculate_sum

# Narrow the search to a specific file
ty-find references my_function -f main.py
```

## See also

- [Commands Overview](overview.md)
