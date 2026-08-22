# MCP server

`tyf mcp` serves the same symbol lookups as the CLI, as an [MCP](https://modelcontextprotocol.io) server over stdio.

**Use MCP where your harness makes MCP easier than shell. The CLI remains the primary interface for Claude Code** — Claude Code already runs shell commands well, and `tyf show MyClass` costs less context than a tool schema injected into every session. Reach for the MCP server when your harness consumes MCP servers more readily than it shells out.

Both frontends talk to the same background daemon, so a warm index is shared between them. Running the MCP server does not give up anything the CLI offers, and it does not duplicate any state.

## Transport

**stdio only.** The harness spawns `tyf mcp`, writes JSON-RPC on its stdin and reads it on its stdout. There is no HTTP transport — tyf is a local tool.

The server runs until stdin closes, then exits. It does not stop the background daemon on exit; the daemon's idle timeout retires it, exactly as after CLI use.

The protocol revision is **2026-07-28**.

## Workspace resolution

The workspace is resolved once, at startup, and used for every tool call:

1. `tyf mcp --workspace <path>` — explicit, wins if present.
2. Otherwise it is auto-detected from the working directory of the `tyf mcp` process, using the same walk-up-for-project-markers logic as the CLI's `--workspace` default: the nearest ancestor with a `pyproject.toml` (or similar marker) wins, falling back to the working directory itself when there is none. Harnesses spawn MCP servers in the project directory, so this is the normal path and usually needs no configuration.

MCP roots are deprecated in the 2026-07-28 revision and are not used.

One server instance serves one workspace. A harness working across several repositories spawns one server per repository.

## Registering the server

### Claude Code

```bash
claude mcp add tyf -- tyf mcp
```

Or in `.mcp.json` at the project root:

```json
{
  "mcpServers": {
    "tyf": {
      "command": "tyf",
      "args": ["mcp"]
    }
  }
}
```

### Codex

In `~/.codex/config.toml`:

```toml
[mcp_servers.tyf]
command = "tyf"
args = ["mcp"]
```

### Cursor

In `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global):

```json
{
  "mcpServers": {
    "tyf": {
      "command": "tyf",
      "args": ["mcp"]
    }
  }
}
```

### Gemini CLI

In `.gemini/settings.json` (project) or `~/.gemini/settings.json` (global):

```json
{
  "mcpServers": {
    "tyf": {
      "command": "tyf",
      "args": ["mcp"]
    }
  }
}
```

If the harness does not spawn the server in your project directory, add the workspace explicitly:

```json
{
  "mcpServers": {
    "tyf": {
      "command": "tyf",
      "args": ["mcp", "--workspace", "/path/to/project"]
    }
  }
}
```

## Tools

One tool per lookup command, with the same parameters and the same semantics. [`calls`](commands/calls.md) has no tool yet — it is gated on `ty` 0.0.41 or newer, so it stays CLI-only until that floor is the norm.

| Tool | Parameters | CLI equivalent |
|------|-----------|----------------|
| `show` | `symbols` (required), `file`, `references`, `doc`, `all` | [`tyf show`](commands/show.md) |
| `find` | `symbols` (required), `file`, `fuzzy` | [`tyf find`](commands/find.md) |
| `refs` | `queries` (required), `file`, `include_declaration` | [`tyf refs`](commands/refs.md) |
| `members` | `symbols` (required), `file`, `all` | [`tyf members`](commands/members.md) |
| `list` | `file` (required) | [`tyf list`](commands/list.md) |

Every symbol and query parameter is an array — pass several symbols in one call rather than one call per symbol.

`refs` accepts symbol names and `file:line:col` positions in the same array, auto-detected per entry, just like the CLI.

Three differences from the CLI are worth knowing:

- `refs`'s `include_declaration` defaults to **false** here; the CLI's `--include-declaration` defaults to true. The tool schema states the default, so an agent reading it sees the same thing you do.
- `show`'s test references follow `all` — there is no separate `tests` parameter.
- There is no `references_limit` parameter: `show` and `refs` display at most 20 individual references, the CLI's default. Output that hits the cap carries the CLI's "use `--references-limit 0` to show all" hint, which an MCP caller cannot act on — narrow the query instead, or use the CLI.

`file` parameters are resolved against the workspace root, not the server's working directory. Absolute paths work too.

### Dotted notation

`Class.member` (one level) narrows to a specific class member, exactly as on the CLI:

```json
{ "symbols": ["Calculator.add"] }
```

Module-qualified names (`module.func`) and nested paths (`Outer.Inner.method`) are not supported. A token with 2+ dots, a leading dot, or a trailing dot is a tool error (`isError: true`) carrying the same message the CLI prints. A well-formed query that simply matches nothing is a normal success result with the "not found" text — the same distinction the CLI draws between exit 2 and exit 0.

## Output

Tool results are **text only** — the same condensed, uncoloured output the CLI produces, with trailing whitespace trimmed. There is no `structuredContent` and no result `outputSchema`: one format, one renderer, shared with the CLI.

## Startup and errors

The background daemon auto-starts on the first tool call, including the restart-on-version-mismatch path, exactly as it does for the CLI. The first call after a cold start pays the index warmup; subsequent calls are fast.

A harness may pipeline tool calls; each is served on its own task. Daemon startup is serialized within the process, so concurrent first calls wait for one daemon rather than each spawning their own.

Errors that reach the bridge — an unreachable daemon, a missing `ty` — come back as tool errors with the same message the CLI would print. See [Troubleshooting](troubleshooting.md).
