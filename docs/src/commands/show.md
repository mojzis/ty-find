# show

Definition, type signature, and usages of a symbol — where it's defined, its type signature, and optionally all usages. Searches the whole project by name, no file path needed.

> **Backward compatibility:** `tyf inspect` still works as a hidden alias for `tyf show`.

Examples:
  tyf show MyClass
  tyf show calculate_sum UserService    # multiple symbols at once
  tyf show MyClass --references         # also show all usages
  tyf show MyClass --doc                # include docstring
  tyf show MyClass --all                # show everything: doc + refs + test refs
  tyf show MyClass --file src/models.py # narrow to one file

## Usage

```
tyf show <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s) to show (supports multiple symbols)

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

**`-r, --references`**
: Also find all references (can be slow on large codebases)

**`-d, --doc`**
: Include the docstring in the output (off by default)

**`-a, --all`**
: Show everything: docstring, references, and test references

## Examples

```bash
# Show a single symbol
tyf show MyClass

# Show multiple symbols at once
tyf show MyClass my_function

# Show a symbol in a specific file
tyf show MyClass --file src/module.py

# Include docstring
tyf show MyClass --doc

# Show everything (doc + refs + test refs)
tyf show MyClass --all

# Using the backward-compatible alias
tyf inspect MyClass
```

## See also

- [Commands Overview](overview.md)
