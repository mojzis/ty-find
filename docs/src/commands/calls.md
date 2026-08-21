# calls

Call tree of a symbol -- recursively, as a tree. Outgoing (the default) answers "what does this do, transitively"; incoming (`--in`) answers "who calls this".

Outgoing replaces a chain of file reads when following a code path. Incoming complements `refs` for impact analysis: `refs` gives raw text ranges, `calls --in` gives the *calling function's identity*, one level per hop.

Use `Class.member` dotted notation (one level only) to narrow to a specific class member. Module-qualified names (`module.func`) and nested paths (`Outer.Inner.method`) are not supported; using 2+ dots is a usage error.

Requires a `ty` with call-hierarchy support (**ty 0.0.41 or newer**). On an older `ty` the command exits **3** with a message naming the installed version — a distinct code from "symbol not found" (which exits 0) and from a usage error (2).

Examples:
  tyf calls process_order                 # what it calls, 2 levels deep
  tyf calls process_order --depth 3       # deeper
  tyf calls check_inventory --in          # who calls it
  tyf calls OrderPipeline.run             # a specific class method
  tyf calls a b c                         # several symbols at once
  tyf calls process_order --external      # locate stdlib callees too

## Usage

```
tyf calls <SYMBOLS> [OPTIONS]
```

## Arguments

**`<symbols>`** *(required)*
: Symbol name(s). Use `Class.member` (one level) to narrow to a class member

## Options

**`--in`**
: Incoming calls: who calls this symbol. Mutually exclusive with `--out`

**`--out`**
: Outgoing calls: what this symbol calls. The explicit form of the default

**`--depth`**
: Recursion depth (default 2, maximum 5; larger values are clamped, not rejected)

**`--external`**
: Show locations for out-of-workspace callees. They are still never expanded

**`-f, --file`**
: Narrow the initial symbol lookup to a specific file

## Examples

```bash
# What does this function do, transitively?
tyf calls process_order

# Go deeper
tyf calls process_order --depth 3

# Who calls this? (impact analysis before an edit)
tyf calls check_inventory --in

# A specific class method
tyf calls OrderPipeline.run

# Several symbols in one invocation
tyf calls process_order charge_payment validate_order

# Locate stdlib/site-packages callees (still not expanded)
tyf calls check_inventory --external

# Add the call-site line numbers
tyf --detail full calls process_order

# Machine-readable
tyf --format json calls process_order
tyf --format csv calls process_order
```

## Output format

The default is a condensed tree, two spaces of indent per level:

```
process_order (src/orders.py:41:1)
  validate_order (src/orders.py:88:1)
    check_inventory (src/inventory.py:12:1)
  charge_payment (src/payments.py:30:1)
    check_inventory ↑
  log.info (external)
```

Markers:

| Marker | Meaning |
|--------|---------|
| `↑` | Already expanded elsewhere in this tree; the subtree is shown at its first occurrence |
| `(cycle)` | Re-enters a definition already on the path from the root (direct or mutual recursion) |
| `(external)` | Defined outside the workspace (stdlib, site-packages); never expanded |

Incoming (`--in`) uses the same tree shape — each level is one caller hop:

```
check_inventory (src/inventory.py:12:1)
  validate_order (src/orders.py:88:1)
    process_order (src/orders.py:41:1)
  charge_payment (src/payments.py:30:1)
    process_order ↑
```

`--detail full` appends the call sites from the LSP `fromRanges`:

```
process_order (src/orders.py:41:1)
  validate_order (src/orders.py:88:1) [called at 43]
```

`--format json` mirrors the tree with the markers as explicit booleans
(`cycle`, `deduped`, `external`), so a consumer never parses the glyphs.
`--format csv` flattens it to `symbol,depth,name,file,line,column,flag` rows.

A symbol that resolves but has no calls is a normal result, not an error: it
prints `(no outgoing calls)` and exits 0.

## Why the tree is pruned

The command exists to cost fewer tokens than reading each file in the chain,
so the walk suppresses repetition rather than reproducing it:

- **Dedup (`↑`)** — a callee reached from two parents (a diamond) is expanded
  at its first occurrence only. Without this, a shared helper's whole subtree
  repeats once per caller.
- **Cycle (`(cycle)`)** — recursion terminates instead of running to the depth
  cap. Direct (`f` calls `f`) and mutual (`a` calls `b` calls `a`) are both
  detected, by definition identity rather than by name.
- **Depth** — capped at 5 in the daemon regardless of what is requested.

## Dotted notation (`Class.member`)

`Class.member` narrows to a member of a specific class (the container is
resolved first, then its members), disambiguating a method name that exists on
several classes. Semantics and limits are identical to `show`, `find`, and
`refs`:

- **One level only** — `Outer.Inner.method` is not supported.
- **Module-qualified names (`module.func`) are not supported.**
- **2+ dots is a usage error** (stderr message + nonzero exit); so is a leading
  or trailing dot (`.foo`, `foo.`).
- A valid dotted query matching nothing exits 0 (normal "not found") with no
  fallback.

## Limitations

- **Calls through callbacks and dynamic dispatch are invisible.** A function
  passed as an argument and invoked by the callee, a method resolved at
  runtime, a `getattr` call — static analysis cannot see these, so they do not
  appear in either direction. `tyf` does not paper over this with heuristics or
  text search: an absent edge means `ty` could not prove one.
- **External code is never expanded, under any flag.** `--external` adds the
  location of a stdlib or site-packages callee; it never walks into it.
- **Depth is capped at 5.** `--depth 99` behaves as `--depth 5` and is not an
  error.
- **A decorator application counts as an outgoing call.** A decorated function
  lists its decorator alongside the callees in its body.
- **A class has no calls.** Construction is not modelled as a call to the
  class, so `tyf calls MyClass` reports no calls. Query a method instead
  (`tyf calls MyClass.method`).
- **Results depend on the installed `ty`.** Call hierarchy requires ty 0.0.41
  or newer; behavior can change between `ty` releases. The observed behavior
  this command is built on is recorded in
  [`docs/dev/call-hierarchy-spike.md`](https://github.com/mojzis/ty-find/blob/main/docs/dev/call-hierarchy-spike.md).

## See also

- [Commands Overview](overview.md)
- [refs](refs.md) -- all usages as raw locations, without caller identity
- [show](show.md) -- definition, type, and usages of any symbol
