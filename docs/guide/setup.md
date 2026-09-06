# ty-find: setup

ty-find (`tyf`) answers "where is X defined", "who uses X" and "what does
X call" for Python code, using ty's language server. It is type-aware, so
unlike grep it does not return string matches, comments, or same-named
functions on unrelated classes.

## Install

Add it as a dev dependency, next to ty:

```bash
uv add --dev ty ty-find
```

Run it through uv so it picks up the project's ty:

```bash
uv run tyf find <symbol>
```

The first real query starts a background daemon that keeps ty's language
server warm; later queries reuse it and return in milliseconds.

## Tell the agent

Add this to the project's CLAUDE.md so symbol questions go to tyf instead
of grep:

```
Use `uv run tyf` instead of grep for Python symbol definitions and references:
- `uv run tyf find <name>`  where a symbol is defined
- `uv run tyf refs <name>`  every usage, before a rename or removal
- `uv run tyf show <name>`  definition and signature
Use grep for string literals, config values, TODOs, and non-Python files.
```

## Try it

```bash
uv run tyf find UserService        # file:line of the definition
uv run tyf refs UserService        # every usage, grouped by file
uv run tyf show UserService.save   # signature and definition of one method
```

A symbol that does not exist is reported on stderr with exit code 0; that
is a clean miss, not an error. Exit 1 is an error, 2 a bad invocation, 3
means the installed ty lacks a feature (upgrade ty).

next: run `uv run tyf guide use`
