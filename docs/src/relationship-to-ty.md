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

<!-- TODO(version-floor): replace this placeholder with a reference to the single
     source of truth for supported ty versions once the version-floor / CI task
     lands. Do not hardcode a second copy of the version list here. -->

> **⚠️ Placeholder — pending the version-floor task.**
> The concrete list of supported `ty` versions is owned by the version-floor / CI
> task as a **single source of truth**. That source is not in this repository yet,
> so **no version numbers are listed here** (and none are invented). When the
> source lands, this section will reference it rather than copy it.

**Support policy.** `tyf` is tested against specific pinned `ty` versions and supports a contiguous range — from a documented floor up to the latest tested release. The exact floor, the latest tested version, and the policy's bound (for example latest-N) are defined by the version-floor task referenced above.

In the meantime, you can always check which `ty` you are running:

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

<!-- Gated: not yet shipped. -->
> _Planned (not yet shipped): running the integration suite against each pinned `ty` version in the supported range, driven by the version-floor task's source of truth._
