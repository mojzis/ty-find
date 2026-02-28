# inspect

Definition, type signature, and usages of a symbol — where it's defined, its type signature, and optionally all usages. Searches the whole project by name, no file path needed.

Examples:
  tyf inspect MyClass
  tyf inspect calculate_sum UserService    # multiple symbols at once
  tyf inspect MyClass --references         # also show all usages
  tyf inspect MyClass --file src/models.py # narrow to one file

## Usage

```
tyf inspect <SYMBOLS> [OPTIONS]
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
tyf inspect MyClass

# Inspect multiple symbols at once
tyf inspect MyClass my_function

# Inspect a symbol in a specific file
tyf inspect MyClass --file src/module.py
```

## See also

- [Commands Overview](overview.md)
