# find

Find symbol definitions by name (searches workspace if no file given)

## Usage

```
ty-find find <SYMBOLS> [OPTIONS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<symbols>` | Symbol name(s) to find (supports multiple symbols) *(required)* |

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | Optional file to search in (uses workspace symbols if omitted) |

## Examples

```bash
# Find a single symbol
ty-find find calculate_sum

# Find multiple symbols at once
ty-find find calculate_sum multiply divide

# Find a symbol in a specific file
ty-find find my_function --file src/module.py
```

## See also

- [Commands Overview](overview.md)
