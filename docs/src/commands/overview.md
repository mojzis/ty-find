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
: Find where a symbol is defined by name

**[type](type.md)**
: Type signature and docs at a file position (line:col)

**[definition](definition.md)**
: Resolve definition at a file position (line:col) — use 'find' for name search

**[refs](refs.md)**
: All usages of a symbol across the codebase

**[list](list.md)**
: All functions, classes, and variables defined in a file

**[workspace-symbols](workspace-symbols.md)**
: Search symbols by name with fuzzy matching (may be merged into find)

**[interactive](interactive.md)**
: Interactive REPL for exploring code

**[daemon](daemon.md)**
: Manage the background LSP server (auto-starts on first use)

