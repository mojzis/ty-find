# hover

Show hover information at a specific file location

## Usage

```
ty-find hover <FILE> [OPTIONS]
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
# Show type information at a location
ty-find hover main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
