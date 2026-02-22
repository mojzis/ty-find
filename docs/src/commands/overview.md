# Commands Overview

Find Python function definitions using ty's LSP server

## Usage

```
ty-find [OPTIONS] <COMMAND>
```

## Global Options

| Option | Description |
|--------|-------------|
| `--workspace` |  |
| `-v, --verbose` |  |
| `--format` |  |

## Commands

| Command | Description |
|---------|-------------|
| [definition](definition.md) | Go to definition at a specific file location |
| [find](find.md) | Find symbol definitions by name (searches workspace if no file given) |
| [interactive](interactive.md) | Interactive REPL for exploring definitions |
| [hover](hover.md) | Show hover information at a specific file location |
| [references](references.md) | Find all references to a symbol at a specific file location |
| [workspace-symbols](workspace-symbols.md) | Search for symbols across the workspace |
| [document-symbols](document-symbols.md) | List all symbols in a file |
| [inspect](inspect.md) | Inspect symbols: find definition, hover info, and references in one shot |
| [daemon](daemon.md) | Manage the background ty LSP server daemon |

