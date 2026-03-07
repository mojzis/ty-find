# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ty-find is a command-line tool that interfaces with ty's LSP server to provide go-to-definition functionality for Python functions, classes, and variables from the terminal. It's a hybrid Rust/Python project that builds a Rust binary (`tyf`) but packages it as a Python package using maturin for easy distribution via pip/uv.

## Prerequisites

- **ty** must be installed and on PATH: `uv add --dev ty` (required for all LSP functionality and integration tests)
- If `ty` is not on PATH, the tool will automatically fall back to running it via `uvx ty`
- **If ty is missing**, install it before running tests: `uv add --dev ty`

## Common Commands

```bash
# Pre-commit checks (always run before committing)
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features

# Run unit tests
cargo test

# Run integration tests (requires ty on PATH)
cargo test --test test_basic

# Build and install locally for testing
maturin develop
```

Run `tyf --help` and `tyf <cmd> --help` for CLI usage examples.

If formatting fails, fix it with `cargo fmt` and re-run the checks.

## Development Workflow

All features and bug fixes follow TDD (red-green-refactor). No implementation code without a failing test first. Bug fixes must include a regression test that fails without the fix.

## CRITICAL: Test Integrity Rules

These rules are NON-NEGOTIABLE. Violating them is worse than not completing the task.

1. **NEVER weaken a test to make it pass.** If a test fails, fix the implementation. If the test itself has a bug, explain what's wrong and get confirmation before changing it.
2. **NEVER delete, skip, or comment out a failing test.** A failing test is a signal. Silencing the signal is not fixing the problem.
3. **NEVER change test assertions to match broken behavior.** If `assert_eq!(result.len(), 5)` fails because result has 3 items, the bug is in the code, not the number 5.
4. **NEVER make error handling more permissive to avoid test failures.** Do not add `.unwrap_or_default()`, catch-all error handlers, or silent fallbacks to make tests pass.
5. **NEVER replace a specific assertion with a weaker one.** Going from `assert_eq!(x, 42)` to `assert!(x > 0)` is test corruption.
6. **Smoke tests and integration tests are sacred.** They test real-world behavior on real repositories. If they fail, the tool is broken. Fix the root cause, never loosen pass criteria.
7. **If you cannot fix a failing test, STOP and report.** Say what the test expects, what actually happens, and what you think is wrong. Do not silently work around it.

## Architecture

### Core Components
1. **LSP Client (`src/lsp/`)** - JSON-RPC client that communicates with ty's LSP server
2. **CLI Interface (`src/cli/`)** - Command-line argument parsing and output formatting
3. **Workspace Detection (`src/workspace/`)** - Python project detection and symbol finding
4. **Main Application (`src/main.rs`)** - Orchestrates the main modes: find, inspect, refs, members, list

Architecture details, patterns, and dependencies: see `docs/dev/ARCHITECTURE.md`.

## Branch Hygiene

**Always merge `main` into your feature branch before creating a PR.** This catches integration issues (compilation errors, test failures) from recently-merged PRs before CI runs. Run:

```bash
git fetch origin main && git merge origin/main
```

Then re-run the full check suite (`cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features`) to verify the merge didn't introduce breakage.

## When Stuck

If hitting a wall (test won't pass, architecture doesn't fit, LSP returns unexpected data):
1. Do not silently work around it — state the problem explicitly.
2. Do not attempt more than 3 approaches without reporting what was tried and why each failed.
3. Do not modify unrelated code hoping it fixes the issue.
4. Revert to last known good state if changes made things worse.

## Review Before Completing Work

Before marking any task as complete, run the review process:

1. **Automated checks** (run automatically via prek pre-commit hook on `git commit`):
   - `cargo fmt --all -- --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all-features --bins`

2. **Deep review** (REQUIRED for all significant changes):
   - You MUST run the `rust-review` skill (`/rust-review`) before marking work as complete or pushing code
   - Address all 🔴 Must Fix items before completing
   - Address 🟡 Should Fix items unless there's a documented reason not to

3. **Full review** (run before pushing):
   - `make review` — runs fmt, clippy, tests, audit, and deny

### Code Rules
- No `.unwrap()` outside tests — use `.context()` when propagating errors with `?`
- No `MutexGuard` held across `.await` — no blocking ops in async without `spawn_blocking`
- Prefer `&str`/`&[T]`/`&Path` over owned types in function parameters when ownership isn't needed
- Tests must assert on values, not just "runs without panic"
- Extract shared logic — don't duplicate LSP message patterns or error handling boilerplate
- Test files are read-only during implementation tasks unless the test itself is the deliverable
- If a smoke/integration test becomes flaky, the fix is in production code or test infra — never in assertions
