# Troubleshooting

## "ty: command not found"

ty-find requires [ty](https://github.com/astral-sh/ty) to be installed and on PATH. Install it with:

```bash
uv add --dev ty
```

If ty is installed but not on PATH, ty-find will attempt to run it via `uvx ty` as a fallback. If neither works, you'll see this error.

## Daemon won't start

Check the daemon status:

```bash
tyf daemon status
```

For more detail, enable debug logging:

```bash
RUST_LOG=ty_find=debug tyf daemon start
```

Common causes:
- Another process is using the daemon's socket.
- ty is not installed (see above).
- Permissions issue on the socket file.

## Wrong or stale results

If tyf returns outdated definitions or missing references, the LSP server may have stale state. Restart the daemon:

```bash
tyf daemon stop && tyf daemon start
```

Then retry your query.

## Slow first call

The first call in a session is expected to be slower because it:

1. Starts the background daemon process.
2. Spawns the ty LSP server.
3. Waits for LSP initialization and project indexing.

Subsequent calls reuse the running daemon and typically respond in 50–100ms. If every call is slow, check that the daemon is staying alive between calls with `tyf daemon status`.

## Empty results for something that should exist

Empty output has two distinct causes — diagnose which one you're hitting before assuming a `tyf` bug.

**(a) The symbol isn't statically visible.** Some dynamic constructs are invisible to static analysis: `getattr`/`setattr` access, runtime-generated classes and methods, attributes assigned through `__init__` magic, and names created by metaprogramming. If the symbol only exists at runtime, `ty` can't see it and neither can `tyf`. First, verify the symbol is in a `.py` file within the workspace.

**(b) `ty` couldn't resolve the environment.** If `ty` is pointed at the wrong interpreter, is missing type stubs, or a third-party package isn't installed, it fails to resolve imports — and unresolved imports cascade into empty navigation results. This looks identical to "not found" but is an environment problem, not a `tyf` problem.

**Diagnostic:** run `ty check` on the file directly:

```bash
ty check path/to/file.py
```

If `ty` reports **unresolved imports** (e.g. "Cannot resolve imported module"), it's an environment problem — fix the interpreter/stubs/dependencies, not your `tyf` query. See how `tyf` behaves with third-party packages and `.venv` resolution in [How It Works](how-it-works.md). If imports resolve cleanly but the symbol still doesn't appear, you're in case (a) above.

## Which ty am I running, and is it supported?

Because `tyf` binds at runtime to whatever `ty` is on your `PATH` (or `uvx ty`), the `ty` version determines your results. Print it with:

```bash
ty --version
# or, if ty is only reachable through uv:
uvx ty --version
```

Compare that against the [Supported ty versions](relationship-to-ty.md#supported-ty-versions) list. `ty` is pre-release (`0.0.x`) and changes often, so if results shift after an upgrade, a version outside the supported range is the first thing to check.

## Signatures show a source annotation or `(unannotated)`

This is expected behavior, not a bug. When `ty` cannot resolve a type — most often because a third-party library's stubs aren't installed — it would normally report `Unknown`. Instead, `tyf` falls back to the literal type annotation as written in the source file (e.g. `-> pa.Table`), reading only the source so it works without the project's dependencies installed. A symbol with no source annotation is shown as `(unannotated)` rather than `Unknown`.

See the "Unresolved types" sections on the [show](commands/show.md#unresolved-types) and [members](commands/members.md#unresolved-types) pages for details.

## Debug logging

For any issue, enable full debug output:

```bash
RUST_LOG=ty_find=debug tyf show MySymbol
```

This shows the LSP messages exchanged with ty, which helps diagnose protocol-level issues.
