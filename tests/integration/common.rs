use std::process;

/// Ensure `ty` is available, either directly on PATH or via `uvx`.
/// Panics with install instructions if neither works.
pub fn require_ty() {
    let direct = process::Command::new("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if direct {
        return;
    }

    let via_uvx = process::Command::new("uvx")
        .arg("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    assert!(
        via_uvx,
        "ty is not installed and uvx fallback failed. Install it with: uv add --dev ty"
    );
}
