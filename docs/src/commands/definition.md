# definition

Go to definition at a specific file location

## Usage

```
ty-find definition <FILE> [OPTIONS]
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

## Examples

```bash
# Find definition at a specific location
ty-find definition main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
