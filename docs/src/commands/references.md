# references

Find every place a symbol is used across the codebase. Useful before renaming or removing code to understand the impact.

Examples:
  ty-find references myfile.py -l 10 -c 5
  ty-find references myfile.py -l 10 -c 5 --no-include-declaration

## Usage

```
ty-find references <FILE> [OPTIONS]
```

## Arguments

**`<file>`** *(required)*
: 

## Options

**`-l, --line`**
: 

**`-c, --column`**
: 

**`--include-declaration`**
: Include the declaration in the results

## Examples

```bash
# Find all references to a symbol
ty-find references main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
