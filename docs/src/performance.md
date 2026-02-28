# Performance

ty-find uses a background daemon to keep the ty LSP server running between calls. The first call starts the daemon and waits for LSP initialization, which is slower. Subsequent calls reuse the running server and respond quickly.

## Benchmarks

Benchmarks are run as part of CI. See `tests/bench_*` for the test code.

| Operation | First call (cold daemon) | Subsequent calls (warm) |
|-----------|-------------------------|------------------------|
| inspect   | TBD                     | TBD                    |
| find      | TBD                     | TBD                    |
| type      | TBD                     | TBD                    |
| refs      | TBD                     | TBD                    |

These numbers will be filled in with real measurements from the benchmark suite.

## What affects performance

- **Project size**: Larger Python projects take longer for the initial LSP indexing.
- **Daemon state**: Cold starts include daemon spawn + LSP initialization. Warm calls skip both.
- **Disk I/O**: First call after a file change triggers re-indexing by the LSP server.
