# Commands Overview

Navigate Python code with type-aware precision (powered by ty's LSP server)

## Usage

```
ty-find [OPTIONS] <COMMAND>
```

## Global Options

**`--workspace`**

**`-v, --verbose`**

**`--format`**

**`--timeout`**
: Timeout in seconds for daemon operations (default: 30)

## Commands

**[inspect](inspect.md)**
: Get the full picture of a symbol: definition, type signature, and usages

**[find](find.md)**
: Jump to where a function, class, or variable is defined by name

**[hover](hover.md)**
: Show type signature and documentation at a specific file location

**[definition](definition.md)**
: Jump to definition from a specific file location (line + column)

**[references](references.md)**
: Find every place a symbol is used across the codebase

**[document-symbols](document-symbols.md)**
: List all functions, classes, and variables defined in a file

**[workspace-symbols](workspace-symbols.md)**
: Search for symbols by name across the whole project

**[interactive](interactive.md)**
: Interactive REPL for exploring definitions

**[daemon](daemon.md)**
: Manage the background LSP server (auto-starts on first use)

