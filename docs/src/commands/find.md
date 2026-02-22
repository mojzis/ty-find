# find

Jump to where a function, class, or variable is defined. Searches the whole project by name — no need to know which file it's in.

Examples:
  ty-find find calculate_sum
  ty-find find calculate_sum multiply divide   # multiple symbols at once
  ty-find find handler --file src/routes.py    # narrow to one file

## Usage

```
ty-find find <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s) to find (supports multiple symbols)

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

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
