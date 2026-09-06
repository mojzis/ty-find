# ty-find: use

Run every command as `uv run tyf ...` (plain `tyf ...` if the venv is
active). Names are searched across the whole project; no file path is
needed. `Class.member` (one dot) narrows to a class member. Several names
at once are fine: `tyf find a b c`.

## Commands

- `tyf find <name>`: where a symbol is defined. First stop for "where is X".
  `--fuzzy` for prefix or partial matches when the exact name is unknown.
- `tyf show <name>`: definition and signature. `-d` docstring, `-r` usages,
  `-t` test usages, `-a` all of them. Read this before editing a function.
- `tyf refs <name>`: every usage, grouped by file. Impact check before a
  rename or removal. `-t` lists test usages separately.
- `tyf members <Class>`: methods, properties and class variables with types.
  `--all` includes private and dunder members.
- `tyf calls <name>`: what it calls, as a tree. `--in` for who calls it.
  `--depth N` (default 2, max 5). Needs ty 0.0.41 or newer.
- `tyf list <file.py>`: everything defined in one file, a table of contents.

## Which one

- Where is it? `find`. Unsure of the spelling? `find --fuzzy`.
- What is it and how is it called? `show`.
- Safe to change or delete? `refs`, then `calls --in`.
- What does it do, transitively? `calls`.
- What does this class expose? `members`.
- What is in this file? `list`.

## Output

The default output is condensed for agents. `--format json` gives
structured output, `--format paths` bare file:line locations. `--file
<path>` narrows a symbol lookup to one file. A symbol that does not exist
is reported on stderr with exit code 0: a clean miss, not an error.

## If something is off

Queries hang, or results look stale after large edits:

```bash
uv run tyf daemon status
uv run tyf daemon stop
```

The next query restarts the daemon. Exit code 3 means the installed ty
lacks the feature; upgrade ty.

next: run `uv run tyf find <symbol>`
