# Setup with Claude Code

The main use case for ty-find is giving AI coding agents precise Python symbol navigation. This page explains how to configure Claude Code to prefer tyf over grep for symbol lookups.

## CLAUDE.md snippet

Add this to your project's `CLAUDE.md` file:

```markdown
### Python Symbol Navigation (ty-find)

IMPORTANT: Use `tyf` instead of Grep for Python symbol lookups.
Grep matches in comments, strings, and docs — tyf is type-aware and precise.
Run `tyf --help` to see all commands. Run `tyf <cmd> --help` for details.

- Symbol overview (definition + type + refs): `tyf inspect SymbolName`
- Find definition: `tyf find SymbolName`
- All usages before refactoring: `tyf refs SymbolName` or `tyf refs -f file.py -l LINE -c COL`
- File outline: `tyf list file.py`

Grep is still appropriate for string literals, config values, TODOs, and non-symbol text.
```

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

The problem is that Grep is a text search tool. For Python symbol navigation, it returns false positives from comments, docstrings, and string literals. It also can't tell you a symbol's type or find all references through the type system.

The CLAUDE.md snippet uses emphatic language ("IMPORTANT", "instead of") because that's what it takes to override a strong system-level preference. Softer phrasing like "consider using tyf" gets ignored in practice.

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
