# show

Definition, type signature, and usages of a symbol — where it's defined, its type signature, and optionally all usages. Searches the whole project by name, no file path needed.

> **Backward compatibility:** `tyf inspect` still works as a hidden alias for `tyf show`.

Use `Class.member` dotted notation (one level only) to narrow to a specific class member. Module-qualified names (`module.func`) and nested paths (`Outer.Inner.method`) are not supported; using 2+ dots is a usage error.

Examples:
  tyf show MyClass
  tyf show MyClass.get_data             # narrow to a specific class method
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
: Symbol name(s) to show. Use `Class.member` (one level) to narrow to a class member.

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

# Show a specific class method (dotted notation)
tyf show MyClass.get_data

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

## Dotted notation (`Class.member`)

`Class.member` narrows a lookup to a member of a specific class — useful when
the same method name (e.g. `get_data`) exists on several classes. The container
is resolved first, then its members are searched, so `Database.get_data`
resolves to `Database`'s member and never `Cache`'s same-named one.

Limitations:

- **One level only.** `Class.member` is supported; nested paths like
  `Outer.Inner.method` are not.
- **Module-qualified names are not supported.** `module.func` will not match —
  dotted notation addresses *class members*, not module paths.
- **Two or more dots is a usage error.** `tyf show a.b.c` prints a message to
  stderr and exits nonzero (it does not attempt a lookup). The same applies to a
  leading or trailing dot (`.foo`, `foo.`).
- A valid dotted query that matches nothing (e.g. `Database.nope` or
  `Nonesuch.get_data`) is a normal "not found" and exits 0 — there is no
  fallback to the bare member or container name.
- No inherited-member resolution: only members defined directly on the class are
  matched (same limitation as [`members`](members.md)).

## Unresolved types

The `Signature` section reports a symbol's type from ty's analysis. When ty
cannot resolve a type — typically because a third-party library's stubs are not
installed — it would normally report `Unknown`. Instead, `tyf` shows the literal
type annotation as written in source (e.g. `def build(self) -> pa.Table`). Only
the source file is read, so this works without the project's dependencies
installed. A symbol with no source annotation is marked `(unannotated)` rather
than `Unknown`.

## See also

- [Commands Overview](overview.md)
