# hover

Show the type signature and documentation for the symbol at a specific position in a file. Useful for understanding what a variable holds, what a function returns, or what a class provides.

Examples:
  ty-find hover src/main.py -l 45 -c 12
  ty-find --format json hover src/main.py -l 45 -c 12   # JSON for scripting

## Usage

```
ty-find hover <FILE> [OPTIONS]
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
# Show type information at a location
ty-find hover main.py --line 10 --column 5
```

## See also

- [Commands Overview](overview.md)
