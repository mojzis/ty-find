# find

Find where a function, class, or variable is defined. Searches the whole project by name — no need to know which file it's in.

Use `Class.member` dotted notation (one level only) to narrow to a specific class member. Module-qualified names (`module.func`) and nested paths (`Outer.Inner.method`) are not supported; using 2+ dots is a usage error.
Use `--fuzzy` for partial/prefix matching (returns richer symbol information including kind and container name).

Examples:
  tyf find calculate_sum
  tyf find Calculator.add                  # find a specific class method
  tyf find calculate_sum multiply divide   # multiple symbols at once
  tyf find handler --file src/routes.py    # narrow to one file
  tyf find handle_ --fuzzy                 # fuzzy/prefix match

## Usage

```
tyf find <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s) to find. Use `Class.member` (one level) to narrow to a class member.

## Options

**`-f, --file`**
: Narrow the search to a specific file (searches whole project if omitted)

**`--fuzzy`**
: Use fuzzy/prefix matching via workspace symbols (richer output with kind + container)

## Examples

```bash
# Find a single symbol
tyf find calculate_sum

# Find a specific class method (dotted notation)
tyf find Calculator.add

# Find multiple symbols at once
tyf find calculate_sum multiply divide

# Find a symbol in a specific file
tyf find my_function --file src/module.py

# Fuzzy/prefix match
tyf find handle_ --fuzzy
```

## Dotted notation (`Class.member`)

`Class.member` narrows to a member of a specific class. The container is
resolved first, then its members are searched, so the same method name on
multiple classes is disambiguated. With `--fuzzy`, the `member` part is matched
per fuzzy/prefix rules while the `container` stays an exact match.

Limitations:

- **One level only** — `Outer.Inner.method` is not supported.
- **Module-qualified names (`module.func`) are not supported.**
- **2+ dots is a usage error** (stderr message + nonzero exit); so is a leading
  or trailing dot (`.foo`, `foo.`).
- A valid dotted query matching nothing exits 0 (normal "not found") with no
  fallback to the bare member or container.

## See also

- [Commands Overview](overview.md)
