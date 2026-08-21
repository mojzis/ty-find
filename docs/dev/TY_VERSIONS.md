# Supported `ty` versions — empirical findings

`tyf` does not ship `ty`; it drives whatever `ty` LSP server is installed on the
user's machine. `ty` is `0.0.x` with frequent releases and breaking changes, so
"green CI against one pinned `ty`" says nothing about the version a user
actually has. This document records the **empirical** evidence behind the
versions `tyf` is tested against, and is the human-readable companion to the
machine-readable source of truth at [`ci/ty-versions.json`](../../ci/ty-versions.json).

> **Single source of truth.** The version list CI tests against lives in
> `ci/ty-versions.json`. The CI matrix and the docs read from it; do not
> duplicate the list elsewhere.

## How the integration suite exercises `ty`

The integration tests (`tests/integration/*.rs`) drive a **real `ty lsp`** over
the wire — there is no mock. Each test runs the real `tyf` binary as a
subprocess via `assert_cmd`; `tyf` spawns the `ty` daemon, which launches
`ty lsp` and speaks JSON-RPC to it. Assertions are made on `tyf`'s rendered
output (definitions, hover/signature, references, workspace/document symbols).
Because the data flows `test → tyf → daemon → ty lsp → back`, a behavior or
format change in `ty` shows up as a test failure. (The only mock in the codebase
is in `src/daemon/client.rs` unit tests, which fake the *daemon* wire protocol
to exercise client logic — those are unit tests, not part of the integration
suite, and intentionally do not touch `ty`.)

## Phase 1 — determinism

A version matrix on top of a flaky suite is noise, so the suite was checked for
byte-stability first, at a fixed `ty` (`0.0.18`, the version the repo pins in
`uv.lock`).

- **The one nondeterminism source.** Across ~55 full-suite reruns at unchanged
  `ty 0.0.18` we saw exactly **one** flake: `test_show_enriched_refs_show_context`
  occasionally rendering `# Refs: none` / `-> (unannotated)`. The same signature
  recurred sporadically during the version sweep (at `0.0.7`, `0.0.45`, …),
  always bracketed by clean passes on adjacent versions — i.e. it is
  **version-independent harness noise**, not a per-version regression.
- **Root cause (real product gaps).** On a freshly spawned daemon, `ty` can
  return an empty result before indexing settles. Almost every query path guards
  against this with `with_warmup` (retry-with-back-off on an empty result) — but
  an audit of `src/daemon/server.rs` found **two** handlers that called the LSP
  client directly with no retry:
  - `handle_inspect`'s reference enrichment (the path `tyf show --references`
    uses) — when the first post-spawn references call came back empty, `show`
    rendered `# Refs: none` (note the ~0.6 s fast failure: no retry budget was
    spent). The standalone `tyf refs` command was unaffected because
    `handle_references` already wraps the call.
  - `handle_members`' `document_symbols` call (the `tyf members` path) — an
    empty symbol set on cold start made a real class look "not found".
- **Fix (product, not harness).** Both now wrap their LSP call in the same
  `with_warmup` retry as the sibling handlers (`handle_references`,
  `handle_document_symbols`). This benefits real users too — `tyf show`/`members`
  no longer under-report right after the daemon starts. The flaky tests
  (`test_show_enriched_refs_show_context`, `test_members_command_non_class_error`)
  are the regression tests. No assertion was relaxed.
- **Result.** Post-fix the suite was rerun with **0 flakes** (see "Verification
  reruns"), including many cold reruns at the versions that flaked most readily.
  Output that could drift across machines is already normalized by `tyf` itself:
  paths are rendered workspace-relative (`uri_to_path` in `src/cli/output.rs`)
  and path lists are sorted.

### Sweep methodology caveat (daemon must be hard-killed between versions)

Switching the installed `ty` version while a daemon is still alive is a trap:
`tyf stop` does not always tear the daemon down promptly, and the daemon **pools
`ty server` child processes per workspace**. A daemon that survives `tyf stop`
keeps serving from its already-spawned (old-version) children — including a
*cached empty* `test_project` result produced by a genuinely-broken old version.
That stale cache then makes **later, perfectly good versions falsely fail** with
"No results found". An early version of our sweep hit exactly this and produced
a misleadingly pessimistic gradient. The fix for the harness is to hard-kill
between versions:

```bash
tyf stop; pkill -f "ty server"; pkill -f "tyf daemon"
rm -f /tmp/ty-find-*.sock /tmp/ty-find-*.pid
```

This is **not** a product or CI concern — each CI matrix job installs one `ty`
and runs the suite once on a fresh runner, so there is no version-switching and
no stale daemon. It only matters when sweeping many versions on one machine.

## Phase 2 — the empirical floor

Every stable `ty` release (`0.0.1` … `0.0.49`) was swept oldest→newest (install
the exact version, hard-kill the daemon, run each integration test binary).

### Genuine capability differences (only in the ancient range)

| ty version(s)     | status        | nature |
|-------------------|---------------|--------|
| `0.0.1`           | not testable  | no glibc `x86_64` wheel published (only musl/i686/armv7l/… + macOS/Windows) — uninstallable on a standard Linux runner |
| `0.0.2` – `0.0.5` | FAIL (genuine)| multi-file **workspace-symbol** lookups return nothing — `find/show Animal` across the multi-module `test_project` → "No results found" (confirmed deterministically with hard-killed daemons) |
| `0.0.6` – `0.0.49`| PASS          | clean; the only failures ever seen here were the version-independent cold-start flake from Phase 1 |

The one genuine capability boundary is multi-file workspace-symbol support: it
is absent on `ty ≤ 0.0.5` and present from `0.0.6` onward.

### No speed or behavior shift across the modern range

Beyond that ancient boundary there is **no** observable difference. Cold-start
(daemon spawn + index) and warm-query latency for `tyf show hello_world --all`
are flat, and the rendered output is **byte-identical**, across the range:

| ty      | cold first-query | warm query | `show` output |
|---------|------------------|------------|---------------|
| `0.0.8` | 190 ms           | 14 ms      | (identical)   |
| `0.0.13`| 178 ms           | 15 ms      | (identical)   |
| `0.0.15`| 180 ms           | 15 ms      | (identical)   |
| `0.0.18`| 178 ms           | 15 ms      | (identical)   |
| `0.0.30`| 178 ms           | 15 ms      | (identical)   |
| `0.0.40`| 171 ms           | 13 ms      | (identical)   |
| `0.0.49`| 173 ms           | 15 ms      | (identical)   |

### Floor

Because nothing in the surface `tyf` exercises changes across `0.0.8`–`0.0.49`,
**the floor is a support-policy choice, not a hard technical limit.** We set it
to a recent, comfortable baseline:

- **Floor: `0.0.15`** (Feb 2026) — well clear of the `≤0.0.5` capability gap and
  the cold-start flake; recent enough that running anything older is unlikely.
- **Latest tested: `0.0.49`.**
- Supported range **`0.0.15` → `0.0.49`**, encoded in
  [`ci/ty-versions.json`](../../ci/ty-versions.json) (`floor`, `latest`, the
  `supported` list the nightly drift job samples, and the curated `matrix`).
  `0.0.18` is kept as a matrix middle because it is the version `uv.lock` pins.

Lowering the floor toward `0.0.6` is safe if older support is ever wanted —
just edit `ci/ty-versions.json`.

### Verification reruns (final harness, hard-killed daemon between runs)

| ty      | flaky reruns | notes |
|---------|--------------|-------|
| `0.0.18` (pre-fix)  | 1 / ~55 | the `# Refs: none` flake |
| `0.0.18` (post-fix) | 0 / 12  | true cold start each run |
| `0.0.49` (post-fix) | 0 / 10  | the version that surfaced the refs flake most readily |

Each post-fix rerun tore the daemon fully down first (`tyf stop`, then kill any
surviving `ty server`/daemon by PID, then remove the socket/pidfile) so every
run is a genuine cold start, like a fresh CI runner. The suite is byte-stable
across ≥10 cold reruns at a fixed `ty`.

> Harness note: `tyf stop` alone does not always tear the daemon down (it can be
> mid-request), and `pkill` is unavailable in some sandboxes — so a teardown
> that only soft-stops will let a daemon survive between version switches and
> serve stale, empty results, making good versions look broken. The PID-based
> teardown above avoids that. CI never hits this (one version, one run, fresh
> runner).

## Phase 3 — the first genuine capability gate above the floor

Everything above concerns behavior that is flat across the supported range. The
`calls` command (LSP call hierarchy) is the first feature that is **not**: it
needs a `ty` newer than the floor.

Swept one version at a time (`initialize` → does the result advertise
`callHierarchyProvider`?), then verified functionally at the boundary:

| ty | `callHierarchyProvider` |
|---|---|
| `0.0.15` … `0.0.40` | **absent** — `textDocument/prepareCallHierarchy` answers JSON-RPC `-32601 Unknown request` |
| `0.0.41` … `0.0.73` | present; `prepare` + `outgoing`/`incoming` all functional and identical at `0.0.41`, `0.0.49`, `0.0.73` |

Full response shapes and per-construct behavior are in
[`call-hierarchy-spike.md`](call-hierarchy-spike.md).

**This does not move the floor.** Every other command still works from `0.0.15`.
Instead:

- The LSP client records `callHierarchyProvider` at `initialize`. When it is
  absent, the `call_hierarchy` daemon RPC returns a structured
  `unsupported_by_ty` error carrying the installed version, and `tyf calls`
  exits `3` with a message naming it — never a silent empty tree, which would
  be indistinguishable from "this function calls nothing".
- `tests/integration/test_calls.rs` gates on the installed version
  (`common::has_call_hierarchy`) and **skips** below `0.0.41`, because the
  *blocking* matrix (`0.0.15`, `0.0.18`, `0.0.33`) deliberately runs versions
  that predate the feature. The suite does run for real on the non-blocking
  `0.0.49` job and on any nightly draw at or above `0.0.41`.
- `uv.lock` pins `ty 0.0.18`, so a local `uv sync` + `cargo test` **skips** the
  `calls` suite (it prints why). To exercise it locally, install a newer `ty`:
  `uv pip install "ty>=0.0.41" --reinstall`.

Whether to raise the floor — or promote a `>= 0.0.41` version to the blocking
matrix so `calls` is gated on every PR — is a call for the version-floor task;
there is a `TODO` pointing here at the capability check in `src/lsp/client.rs`.

## Curated CI matrix

The CI matrix (`.github/workflows/ty-version-matrix.yml`) does **not** run the
full sweep on every push. It runs a curated subset read from
`ci/ty-versions.json`:

- **Blocking** (per push/PR): floor + middles (`0.0.15`, `0.0.18`, `0.0.33`). A
  regression here fails the PR.
- **Non-blocking** (per push/PR): latest (`0.0.49`), `continue-on-error` so a
  fresh `ty` release pings us without blocking unrelated PRs.
- **Nightly drift** (cron, non-blocking): floor + latest + one **random**
  in-between version drawn fresh each run from the `supported` list, so over a
  week of nightlies the whole range is covered without making the per-PR gate
  non-deterministic.

Linux only: the daemon needs a Unix domain socket, so Windows is out of scope
(only `tyf find --file` works there); macOS is covered today by the release-build
workflow.

## Reproducing the sweep

```bash
cargo test --no-run --all-features      # build test binaries once

for v in <versions>; do
  uv tool install "ty==$v" --force
  tyf stop; pkill -f "ty server"; rm -f /tmp/ty-find-*.sock /tmp/ty-find-*.pid
  cargo test --test test_basic --test test_complex_project \
             --test test_daemon --test test_multi_workspace \
             --test test_project_smoke
done
```
