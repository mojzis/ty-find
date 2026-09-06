# guide

Agent-facing instructions, embedded in the binary so they always match the build. Prints one page and exits; never touches the workspace, the daemon or the LSP, so it is safe as the very first command in a fresh checkout — including `uvx ty-find guide`, which needs nothing installed.

With no page given, picks one from the working directory: `use` when the nearest `pyproject.toml` has a `.venv` with `tyf` installed, `setup` otherwise. Both pages are always reachable by name.

The pages live in `docs/guide/` and are compiled into the binary with `include_str!`, so the text always matches the build. Each page is under 60 lines, plain ASCII, and ends with a single `next:` line telling the reader what to run.

## Usage

```
tyf guide [PAGE]
```

## Arguments

**`<page>`**
: Page to print: `setup` or `use` (default: detect from the working directory)

## Examples

```bash
uvx ty-find guide      # from anywhere, nothing installed
tyf guide              # auto-select
tyf guide setup        # install, wire into CLAUDE.md, run a first query
tyf guide use          # command reference: which command answers which question
```

## See also

- [Commands Overview](overview.md)
- [Setup with Claude Code](../setup.md)
