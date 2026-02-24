---
name: rust-review
context: fork
description: Deep Rust code quality review. Auto-invoke when finishing a task, before marking work complete, when the user asks to review code, or when preparing a PR. Covers error handling, async correctness, duplicated logic, test quality, performance patterns, and idiomatic Rust beyond what clippy catches.
---

# Deep Code Review for ty-find

Perform a thorough code quality review of the changes in this project. Go beyond what clippy and rustfmt catch. Focus on the areas below and report findings grouped by severity: 🔴 Must Fix, 🟡 Should Fix, 🟢 Suggestion.

First, run `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` to confirm the automated checks pass. If they don't, fix those first before proceeding with the deep review.

## 1. Error Handling Quality

- `.unwrap()` outside tests is forbidden. Flag any instance.
- Check that `.context("message")` is used when propagating errors from external crates with `?` — bare `?` loses context about what operation failed.
- Flag `Result` return types where the function never actually returns `Err` (remove the wrapper — clippy catches this as `unnecessary_wraps` but double-check).
- Flag `.map_err(|_| ...)` that silently discards error information.
- Verify error types are specific enough — no `String` as error type, no `anyhow::Error` leaking into library-style modules.
- Check that `thiserror` error variants have `#[from]` or manual `From` impls for their sources.

## 2. Async / Tokio Correctness

- Flag any `MutexGuard` or `RwLockGuard` held across an `.await` point — this can deadlock or prevent `Send` futures.
- Flag blocking operations (std::fs, std::process::Command, heavy computation) running directly in async context without `spawn_blocking`.
- Check that spawned tasks (`tokio::spawn`) have their `JoinHandle` either awaited or explicitly dropped with a comment explaining why.
- Verify `tokio::select!` branches handle cancellation correctly (partially completed work).
- Check for unbounded channels or queues that could cause memory issues under load.

## 3. Duplicated Logic

- Search for functions or code blocks that do substantially the same thing with minor variations. Flag them and suggest extraction into a shared function, trait, or generic.
- Pay special attention to:
  - LSP message construction patterns (likely repeated across commands)
  - Error handling boilerplate that could be a helper
  - Serialization/deserialization patterns
  - Similar match arms across different functions
- Suggest concrete refactoring: which function to extract, what parameters it should take.

## 4. Test Quality

- **Tests that don't assert**: Flag any test that just calls code without asserting on the result. Running without panicking is not a test.
- **Tests that assert too little**: `assert!(result.is_ok())` without checking the value inside is weak. Suggest asserting on the actual content.
- **Overly verbose tests**: Tests with excessive setup that obscure what's actually being tested. Suggest extracting test helpers or fixtures.
- **Missing edge cases**: For each public function, check if tests cover: empty input, error paths, boundary conditions.
- **Test naming**: Test names should describe the behavior being tested, not the implementation (`test_returns_error_on_missing_file` not `test_function_xyz`).
- **Integration vs unit**: Verify that unit tests (in `#[cfg(test)] mod tests`) test internal logic, while `tests/integration/` tests exercise the public CLI interface.
- Suggest adding `proptest` for any function that transforms data (roundtrip properties).

## 5. Performance Patterns

- Flag unnecessary `.clone()` — suggest borrowing or restructuring ownership.
- Flag `.collect::<Vec<_>>()` immediately followed by `.iter()` — the intermediate collection is usually unnecessary.
- Prefer `&str` over `String`, `&[T]` over `Vec<T>`, `&Path` over `PathBuf` in function parameters when ownership isn't needed.
- Flag large structs being passed by value instead of reference.
- Check for `Arc::clone(&x)` vs `x.clone()` — the former is required by our lint config for clarity.
- In async code, flag futures that capture large amounts of data across await points.

## 6. Idiomatic Rust

- Prefer iterator chains over manual `for` loops with `push`.
- Use `if let` / `let else` instead of `match` with a single interesting arm and a wildcard.
- Suggest `impl From<X> for Y` instead of standalone conversion functions.
- Suggest `Display` implementations instead of `to_string()` methods.
- Flag `&String`, `&Vec<T>`, `&Box<T>` in function signatures — use `&str`, `&[T]`, `&T` instead.
- Check doc comments on all public types and functions.

## 7. API and Module Design

- Is `main.rs` thin? It should just parse args and call into library code.
- Are module boundaries clean? Each module should have a clear responsibility.
- Check for any `pub` items that don't need to be public.
- Flag circular dependencies between modules.

## Output Format

Group all findings by severity, then by area. For each finding:
- State **what** the issue is and **where** (file:line or function name)
- Explain **why** it matters
- Suggest a **concrete fix** (not just "improve this")

End with a summary: X must-fix, Y should-fix, Z suggestions.

$ARGUMENTS
