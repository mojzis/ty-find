# mcp

Serve the same commands as an MCP server over stdio, for harnesses that consume MCP servers more easily than a shell command.

Exposes one tool per lookup command — `show`, `find`, `refs`, `members`, `list` — with the same parameters, the same dotted-notation rules, and byte-identical condensed output. [`calls`](calls.md) is CLI-only for now. It is a thin bridge over the same background daemon the CLI uses, so both frontends share one warm index.

Runs until stdin closes; the harness starts and stops it. stdio is the only transport. The workspace is resolved once at startup from `--workspace`, or auto-detected from the process's working directory when that is omitted.

See [MCP server](../mcp.md) for registration snippets, the full tool surface, and how MCP compares to the CLI.

## Usage

```
tyf mcp [OPTIONS]
```

## Options

**`--workspace <PATH>`**
: Project root for every tool call (default: auto-detect from the working directory)

## Examples

```bash
# Workspace auto-detected from the current directory — what a harness normally spawns
tyf mcp

# Explicit workspace, for a harness that spawns servers elsewhere
tyf mcp --workspace /path/to/project
```

## See also

- [MCP server](../mcp.md)
- [Commands Overview](overview.md)
