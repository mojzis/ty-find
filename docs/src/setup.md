# Setup with Claude Code

The main use case for ty-find is giving AI coding agents precise Python symbol navigation. This page explains how to configure Claude Code to prefer tyf over grep for symbol lookups.

## CLAUDE.md snippet

Add this to your project's `CLAUDE.md` file:

<!-- BEGIN SHARED:claude-snippet -->
```markdown
### Python Symbol Navigation — `tyf`

This project has `tyf` — a type-aware code search that gives LSP-quality
results by symbol name. Use `tyf` instead of grep/ripgrep for Python symbol lookups.

| Task | Command |
|------|---------|
| Definition + signature | `tyf show my_function` |
| ...with docstring | `tyf show my_function --doc` |
| ...with all details | `tyf show my_function --all` |
| Find definition | `tyf find MyClass` |
| All usages (before refactoring) | `tyf refs my_function` |
| Class public API | `tyf members TheirClass` |
| File outline | `tyf list file.py` |

All commands accept multiple symbols — batch to save tool calls.
Run `tyf <cmd> --help` for options.

Use grep for: string literals, config values, TODOs, non-Python files.
```
<!-- END SHARED:claude-snippet -->

## Permissions

Claude Code will prompt you for permission the first time it tries to run `tyf`. To avoid repeated prompts, add a Bash permission rule in your project's `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": [
      "Bash(tyf:*)"
    ]
  }
}
```

This allows Claude Code to run any `tyf` command without asking each time.

## Why the strong language?

Claude Code's system prompt tells it to use its built-in Grep tool for searching code. This is a sensible default — Grep works everywhere and requires no setup.

The problem goes deeper than precision. To use an LSP (the gold standard for code intelligence), an LLM first needs a file position — but it doesn't know the position without searching. So it greps, gets imprecise results, and has to validate them — a circular round-trip that wastes tokens and time.

tyf breaks this cycle: the LLM passes a symbol name, and tyf resolves the position internally, returning structured LSP results directly. No grep step, no position guessing, no validation loop.

On top of that, grep is a text search tool — it returns false positives from comments, docstrings, and string literals. It can't tell you a symbol's type or find all references through the type system.

The CLAUDE.md snippet uses emphatic language ("Use `tyf` instead of grep") because that's what it takes to override a strong system-level preference. Softer phrasing like "consider using tyf" gets ignored in practice.

## Priming a new session

In the first Claude Code session with a new project, you can prime Claude by asking it to run:

```
tyf --help
```

This helps Claude understand what commands are available and how to use them, making it more likely to reach for tyf over grep in subsequent interactions.

## AGENTS.md for other tools

If you use Cursor, Codex, Gemini CLI, or another AI coding tool, the same instructions work — just put them in the file your tool reads:

| Tool | File |
|------|------|
| Claude Code | `CLAUDE.md` |
| Cursor | `.cursorrules` |
| Codex | `AGENTS.md` |
| Gemini CLI | `GEMINI.md` |

If you use multiple tools, you can maintain one file and symlink:

```bash
# Write instructions in CLAUDE.md, symlink for others
ln -s CLAUDE.md AGENTS.md
ln -s CLAUDE.md .cursorrules
```
