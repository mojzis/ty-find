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

**`--timeout`**
: Timeout in seconds for daemon operations (default: 30)

## Commands

**[inspect](inspect.md)**
: Definition, type signature, and usages of a symbol by name

**[find](find.md)**
: Find where a symbol is defined by name (--fuzzy for partial matching)

**[refs](refs.md)**
: All usages of a symbol across the codebase (by name or file:line:col)

**[members](members.md)**
: Public interface of a class: methods, properties, and class variables

**[list](list.md)**
: All functions, classes, and variables defined in a file

**[daemon](daemon.md)**
: Manage the background LSP server (auto-starts on first use)
