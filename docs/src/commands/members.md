# members

Public interface of a class -- methods with signatures, properties, and class variables with types. Like 'list' scoped to a class, with type info included.

Excludes private (_prefixed) and dunder (__dunder__) members by default; use --all to include everything.

Note: only shows members defined directly on the class, not inherited members.

Examples:
  tyf members MyClass
  tyf members MyClass UserService        # multiple classes
  tyf members MyClass --all              # include __init__, __repr__, etc
  tyf members MyClass -f src/models.py   # narrow to one file

## Usage

```
tyf members <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Class name(s) to inspect (supports multiple classes)

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

**`--all`**
: Include dunder methods and private members (excluded by default)

## Examples

```bash
# Show public interface of a class
tyf members MyClass

# Multiple classes at once
tyf members MyClass UserService

# Include dunder methods and private members
tyf members MyClass --all

# Narrow to a specific file
tyf members MyClass --file src/models.py
```

## Output format

The default text output groups members by category:

```
MyClass (src/models.py:15:1)
  Methods:
    calculate_total(self, items: list[Item]) -> Decimal    :42:5
    validate(self) -> bool                                 :58:5
  Properties:
    name: str                                              :20:5
    is_active: bool                                        :23:5
  Class variables:
    MAX_RETRIES: int = 3                                   :16:5
```

Line/col references on the right allow jumping to the source.

## Limitations

- Only shows members defined directly on the class, not inherited members (MRO traversal is not yet supported by ty's LSP)
- Type signatures come from hover, so they require ty to have analyzed the file

## See also

- [Commands Overview](overview.md)
- [inspect](inspect.md) -- for definition, type, and usages of any symbol
- [list](list.md) -- for all symbols in a file
