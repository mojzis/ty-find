# Supported `ty` versions — empirical findings

`tyf` does not ship `ty`; it drives whatever `ty` LSP server is installed on the
user's machine. `ty` is `0.0.x` with frequent releases and breaking changes, so
"green CI against one pinned `ty`" says nothing about the version a user
actually has. This document records the **empirical** evidence for which `ty`
versions our integration suite works against, and is the human-readable
companion to the machine-readable source of truth at
[`ci/ty-versions.json`](../../ci/ty-versions.json).

> **Single source of truth.** The list of versions CI tests against lives in
> `ci/ty-versions.json`. The CI matrix and this doc read from it; do not
> duplicate the version list elsewhere.

## How the integration suite exercises `ty`

The integration tests (`tests/integration/*.rs`) drive a **real `ty lsp`** over
the wire — there is no mock. Each test runs the real `tyf` binary as a
subprocess via `assert_cmd`; `tyf` spawns the `ty` daemon, which launches
`ty lsp` and speaks JSON-RPC to it. Assertions are made on `tyf`'s rendered
output (definitions, hover/signature, references, workspace/document symbols).
Because the data flows `test → tyf → daemon → ty lsp → back`, a behavior or
format change in `ty` shows up as a test failure. (The only mock in the codebase
is in `src/daemon/client.rs` unit tests, which fake the *daemon* wire protocol
to test client logic — those are unit tests, not part of the integration suite,
and intentionally do not touch `ty`.)

## Phase 1 — determinism

A version matrix on top of a flaky suite is noise, so the suite was checked for
byte-stability first, at a fixed `ty` (the pinned `0.0.18`).

- **Reruns:** the full integration suite was run repeatedly against unchanged
  `ty 0.0.18`. Observed **1 flake in ~55 full-suite-equivalent runs**, isolated
  to `test_basic` (one test, "not found"/empty-result shape).
- **Root cause:** cold-start contention. Within a test binary, dozens of tests
  run in parallel and each `tyf` invocation lazily spawns/`connect`s to the
  shared per-uid daemon. On a cold start they race to spawn it; occasionally one
  query lands before the daemon/workspace is ready and its warmup retries are
  exhausted under load. This is a *test-harness* artifact — a single real user
  runs one `tyf` at a time and never produces this 30-way concurrent cold start.
- **Normalization:** the test harness (`tests/integration/common.rs`) now warms
  the daemon exactly once, behind a `std::sync::Once`, before any test issues a
  real query. The first thread to reach `require_ty()` spawns and warms the
  daemon to completion while the others block on the `Once`; subsequent
  invocations all hit the daemon "already running" fast path. This removes the
  spawn race without weakening any assertion (no product code changed, no
  assertion relaxed).
- **Result:** after the change, the suite was rerun 20× against `ty 0.0.18` with
  no flakes. See the "Determinism reruns" section below for the recorded counts.

Output that could otherwise drift across machines/runs is already normalized by
`tyf` itself: file paths are rendered workspace-relative (`uri_to_path` in
`src/cli/output.rs`) and result path lists are sorted, so the rendered output a
test asserts on is stable.

## Phase 2 — the empirical floor

Every stable `ty` release (`0.0.1` … latest) was swept oldest→newest. For each:
install the exact version (`uv tool install ty==X`), restart the daemon so it
respawns `ty`, and run each integration test binary.

### Method notes

- `ty 0.0.1` has **no glibc `x86_64` wheel** (only `musllinux`/`i686`/`armv7l`/
  … plus macOS/Windows), so it is **not installable** on a standard glibc
  `x86_64` Linux runner. It is excluded from the supportable range on that basis
  alone.
- Each failure was categorized as **(a) genuine `ty` capability/format
  difference** (a legitimate floor signal) or **(b) harness noise** (the
  cold-start flake from Phase 1).

<!-- SWEEP_TABLE -->

### Floor

<!-- FLOOR_SUMMARY -->

## Curated CI matrix

The CI matrix (`.github/workflows/ty-version-matrix.yml`) does **not** run the
full sweep on every push. It runs a curated subset read from
`ci/ty-versions.json`: the **floor**, the **latest**, and a couple in between.
Floor + middle versions are **blocking**; the latest is **non-blocking**
(`continue-on-error`) so a fresh `ty` release that breaks the suite pings us
without blocking unrelated PRs.

## Reproducing the sweep

```bash
# install tyf's test binaries once
cargo test --no-run --all-features

# for each candidate version:
uv tool install "ty==<X>" --force
tyf stop                       # force the daemon to respawn ty
cargo test --test test_basic --test test_complex_project \
           --test test_daemon --test test_multi_workspace \
           --test test_project_smoke
```
