use std::path::PathBuf;
use std::process;
use std::sync::Once;

/// Ensure `ty` is available and the shared daemon is warm before a test issues
/// its real query.
///
/// Every integration test calls this first. It (1) checks `ty` is installed
/// (directly on PATH or via `uvx`) and (2) warms the daemon exactly once via
/// [`ensure_daemon_warm`].
pub fn require_ty() {
    let direct = process::Command::new("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let available = direct
        || process::Command::new("uvx")
            .arg("ty")
            .arg("--version")
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    assert!(
        available,
        "ty is not installed and uvx fallback failed. Install it with: uv add --dev ty"
    );

    ensure_daemon_warm();
}

/// Warm the shared daemon exactly once before the parallel test storm.
///
/// Integration tests run in parallel and each `tyf` invocation lazily
/// spawns/connects to the shared per-uid daemon. On a cold start dozens of them
/// race to spawn it, and occasionally a query lands before the daemon/workspace
/// has finished indexing — manifesting as empty references or an unresolved
/// hover (`# Refs: none`, `-> (unannotated)`). A single real user runs one
/// `tyf` at a time and never produces this many-way concurrent cold start, so
/// this is a test-harness artifact, not a product bug.
///
/// Gating one warmup query behind a [`Once`] serializes the cold start: the
/// first test to reach here spawns and warms the daemon to completion (indexing
/// the shared `example.py` fixture and computing references for `hello_world`)
/// while the other test threads block on the `Once`. Afterwards every `tyf`
/// invocation hits the daemon "already running" fast path against a warm
/// workspace. This removes the spawn/index race without weakening any assertion
/// and without touching product code.
pub fn ensure_daemon_warm() {
    static WARMUP: Once = Once::new();
    WARMUP.call_once(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let example = manifest_dir.join("example.py");
        // Mirror the heaviest first-query path (definition + hover + references)
        // so the daemon is fully warm. Best-effort: ignore the result — if this
        // fails, the individual test will surface the real error.
        let _ = process::Command::new(env!("CARGO_BIN_EXE_tyf"))
            .arg("--workspace")
            .arg(&manifest_dir)
            .arg("show")
            .arg("hello_world")
            .arg("--file")
            .arg(&example)
            .arg("--references")
            .arg("--references-limit")
            .arg("0")
            .output();
    });
}
