# definition

Resolve where a symbol is defined, given its exact location in a file. Use this when you already know the file, line, and column (e.g., from an editor). For name-based search, use 'find' or 'inspect' instead.

Examples:
  tyf definition myfile.py -l 10 -c 5

## Usage

```
tyf definition <FILE> [OPTIONS]
```

## Arguments

**`<file>`** *(required)*
:

## Options

**`-l, --line`**
:

**`-c, --column`**
:

## Examples

```bash
# Find definition at a specific location
tyf definition main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
