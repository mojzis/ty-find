# Commands Overview

Type-aware Python code navigation (powered by ty)

## Usage

```
tyf [OPTIONS] <COMMAND>
```

## Global Options

**`--workspace`**
: Project root (default: auto-detect)

**`-v, --verbose`**
: Enable verbose output

**`--format`**
: Output format: human (default), json, csv, or paths

**`--detail`**
: Output detail level: condensed (token-efficient, default) or full (verbose)

**`--timeout`**
: Timeout in seconds for daemon operations (default: 30)

**`--color`**
: When to use colored output: auto (default), always, or never. Respects the `NO_COLOR` environment variable.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success — including a well-formed query that matched nothing ("not found" is a normal result, not an error) |
| `1` | Runtime error (daemon unreachable, unreadable file, `ty` not installed) |
| `2` | Usage error (malformed invocation, e.g. 2+ dots in a dotted symbol) |
| `3` | The installed `ty` lacks a capability the command needs (see [calls](calls.md)) |

## Commands

**[show](show.md)**
: Definition, type signature, and usages of a symbol by name

**[find](find.md)**
: Find where a symbol is defined by name (--fuzzy for partial matching)

**[refs](refs.md)**
: All usages of a symbol across the codebase (by name or file:line:col)

**[members](members.md)**
: Public interface of a class: methods, properties, and class variables

**[calls](calls.md)**
: Call tree of a symbol: what it calls (`--in` for what calls it)

**[list](list.md)**
: All functions, classes, and variables defined in a file

**[daemon](daemon.md)**
: Manage the background LSP server (auto-starts on first use)
