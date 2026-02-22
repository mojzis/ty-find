# inspect

Get the full picture of a symbol — where it's defined, its type signature, and optionally all usages. Searches the whole project by name, no file path needed.

Examples:
  ty-find inspect MyClass
  ty-find inspect calculate_sum UserService    # multiple symbols at once
  ty-find inspect MyClass --references         # also show all usages
  ty-find inspect MyClass --file src/models.py # narrow to one file

## Usage

```
ty-find inspect <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s) to inspect (supports multiple symbols)

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

**`-r, --references`**
: Also find all references (can be slow on large codebases)

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
