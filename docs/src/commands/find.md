# find

Find where a function, class, or variable is defined. Searches the whole project by name — no need to know which file it's in.

Use `--fuzzy` for partial/prefix matching (returns richer symbol information including kind and container name).

Examples:
  tyf find calculate_sum
  tyf find calculate_sum multiply divide   # multiple symbols at once
  tyf find handler --file src/routes.py    # narrow to one file
  tyf find handle_ --fuzzy                 # fuzzy/prefix match

## Usage

```
tyf find <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s) to find (supports multiple symbols)

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

**`--fuzzy`**
: Use fuzzy/prefix matching via workspace symbols (richer output with kind + container)

## Examples

```bash
# Find a single symbol
tyf find calculate_sum

# Find multiple symbols at once
tyf find calculate_sum multiply divide

# Find a symbol in a specific file
tyf find my_function --file src/module.py

# Fuzzy/prefix match
tyf find handle_ --fuzzy
```

## See also

- [Commands Overview](overview.md)
