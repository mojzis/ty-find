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

## No results for a symbol that exists

- Verify the symbol is in a `.py` file within the workspace.
- Check that ty can analyze the file: `ty check file.py`.
- Some dynamic constructs (e.g., `getattr`, runtime-generated classes) are not visible to static analysis.

## Debug logging

For any issue, enable full debug output:

```bash
RUST_LOG=ty_find=debug tyf inspect MySymbol
```

This shows the LSP messages exchanged with ty, which helps diagnose protocol-level issues.
