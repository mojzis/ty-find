# inspect

Inspect symbols: find definition, hover info, and references in one shot

## Usage

```
ty-find inspect <SYMBOLS> [OPTIONS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<symbols>` | Symbol name(s) to inspect (supports multiple symbols) *(required)* |

## Options

| Option | Description |
|--------|-------------|
| `-f, --file` | Optional file to narrow the search (uses workspace symbols if omitted) |

## Examples

```bash
# Inspect a single symbol
ty-find inspect MyClass

# Inspect multiple symbols at once
ty-find inspect MyClass my_function

# Inspect a symbol in a specific file
ty-find inspect MyClass --file src/module.py
```

## See also

- [Commands Overview](overview.md)
