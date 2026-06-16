# Relationship to ty

`tyf` is an **adapter over [ty](https://github.com/astral-sh/ty)'s LSP server**. It does no Python analysis of its own — every definition, signature, and reference it returns comes from `ty`.

## tyf does not ship ty

`ty` is a **required peer dependency that you install yourself**. `tyf` does not bundle, vendor, or pin a copy of `ty`. At runtime it binds to whatever `ty` it finds:

1. `ty` on your `PATH` (preferred), or
2. `uvx ty` as a fallback.

The analysis you get therefore depends on the `ty` version in your environment, not on the `tyf` version. Upgrading `tyf` does not upgrade `ty`, and upgrading `ty` does not upgrade `tyf`.

```bash
# Install ty yourself (required)
uv add --dev ty
```

## Why this matters: ty is pre-release

`ty` is **pre-release software (`0.0.x`)** under active development, with frequent breaking changes — including to LSP and diagnostic behavior. Because `tyf` binds to your installed `ty` at runtime, a `ty` upgrade can change `tyf`'s output without any change to `tyf` itself.

Practical guidance:

- Keep `ty` current **within the supported range** (see [Supported ty versions](#supported-ty-versions)).
- If results change after a `ty` upgrade, check your `ty` version against the supported list before assuming a `tyf` bug — see [Which ty am I running, and is it supported?](troubleshooting.md#which-ty-am-i-running-and-is-it-supported).

## What tyf surfaces (and what it doesn't)

`tyf` surfaces `ty`'s **navigation and symbol knowledge** — definitions, type signatures, references, and class members — **not** `ty`'s type-checking diagnostics. `tyf` never reports type errors, so the false-positive noise that affects `ty check` on dynamic frameworks (Pydantic, SQLAlchemy, and similar) does **not** appear in `tyf` output.

## Supported ty versions

`tyf` is tested against a contiguous range of **pinned, exact** `ty` versions, from a documented floor up to the latest tested release:

- **Floor:** `0.0.15`
- **Latest tested:** `0.0.49`

This is a *practical* baseline, not a hard technical limit. The integration suite behaves identically — same output, same ~180ms cold start — across the whole modern range; the only genuine capability gaps are in `ty ≤ 0.0.5` (no multi-file workspace symbols), and `0.0.1` has no Linux glibc wheel at all.

> **Single source of truth.** The exact list of tested versions, the floor, and the latest tested release live in [`ci/ty-versions.json`](https://github.com/mojzis/ty-find/blob/main/ci/ty-versions.json). The CI matrix and the docs read from it — the version numbers above are a convenience summary, not a second copy of the list. The methodology and per-version findings are in [`docs/dev/TY_VERSIONS.md`](https://github.com/mojzis/ty-find/blob/main/docs/dev/TY_VERSIONS.md).

**Support policy.** `tyf` supports the range from the floor up to the latest tested version. Because `ty` is pre-release and changes often, a `ty` version outside this range may produce different output (or none) without that being a `tyf` bug — check your version first when results shift after an upgrade.

You can always check which `ty` you are running:

```bash
ty --version
# or, if ty is only reachable through uv:
uvx ty --version
```

## Testing

`tyf`'s integration tests drive a **real `ty lsp` process** — `ty` is never mocked. Each test spawns the actual `tyf` binary, which starts a real daemon and a real `ty` LSP server, then asserts on the structured output for known fixtures.

Fixtures live at the repo root:

| Fixture | Purpose |
|---------|---------|
| `example.py` | Minimal single-file fixture used by the basic suite |
| `test_project/` | Multi-file project exercising classes, generics, protocols, enums, async code, decorators, and exceptions |
| `test_project2/` | A second workspace, used to test multi-workspace daemon behavior |

The integration suites live in `tests/integration/`: `test_basic.rs`, `test_complex_project.rs`, `test_daemon.rs`, `test_multi_workspace.rs`, and `test_project_smoke.rs`.

### Running the tests locally

`ty` must be installed and on `PATH` (or reachable via `uvx ty`). Tests that need it call `require_ty()` and fail fast with install instructions if it is missing.

```bash
# Install ty (required for integration tests)
uv add --dev ty

# Run everything (unit + integration)
cargo test --all-features

# Run just the basic integration suite
cargo test --test test_basic
```

CI runs the same `cargo test --all-features` suite with `ty` installed, plus a separate smoke-test workflow (`benchmarks/smoke.sh`) against the release binary.

### Testing across ty versions

Because `tyf` binds to whatever `ty` you have installed, green CI against a single pinned `ty` proves little about the version a user actually runs. So the integration suite is also run against a **curated set of pinned, exact `ty` versions** by the [`ty-version-matrix`](https://github.com/mojzis/ty-find/blob/main/.github/workflows/ty-version-matrix.yml) workflow. The version list is read from [`ci/ty-versions.json`](https://github.com/mojzis/ty-find/blob/main/ci/ty-versions.json) (the single source of truth), so it lives in exactly one place. The matrix has three tiers:

| Tier | When | Versions | Effect |
|------|------|----------|--------|
| **Blocking** | every push / PR | floor + middle versions (`0.0.15`, `0.0.18`, `0.0.33`) | a regression here fails the PR |
| **Latest (non-blocking)** | every push / PR | latest tested (`0.0.49`) | `continue-on-error` — a fresh `ty` release that breaks the suite pings us without blocking unrelated PRs |
| **Nightly drift (non-blocking)** | scheduled (cron) | floor + latest + one *random* in-between version | over a week of nightlies the whole supported range is covered, while the per-PR gate stays deterministic |

This also catches dependabot `ty` bumps before merge. The full sweep methodology, the empirical floor rationale, and per-version findings are in [`docs/dev/TY_VERSIONS.md`](https://github.com/mojzis/ty-find/blob/main/docs/dev/TY_VERSIONS.md).

Linux only: the daemon needs a Unix domain socket, so the matrix does not cover Windows (only `tyf find --file` works there); macOS is covered by the release-build workflow.
