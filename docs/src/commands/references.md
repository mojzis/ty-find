# references

Find all references to a symbol at a specific file location

## Usage

```
ty-find references <FILE> [OPTIONS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<file>` |  *(required)* |

## Options

| Option | Description |
|--------|-------------|
| `-l, --line` |  |
| `-c, --column` |  |
| `--include-declaration` | Include the declaration in the results |

## Examples

```bash
# Find all references to a symbol
ty-find references main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
