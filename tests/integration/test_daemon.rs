#![cfg(unix)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Test that `daemon start` does not fork-bomb.
///
/// Strategy: run the binary with `daemon start` while the daemon socket is
/// absent.  If the bug is present the process will recursively spawn copies of
/// itself.  We give it a short window (2 s) then count how many `ty-find`
/// processes exist.  A correct implementation spawns at most **2** processes
/// (the original CLI invocation + one background child).  A fork bomb would
/// create dozens or hundreds.
///
/// Because the real daemon needs a working `ty` LSP server (which may not be
/// available in CI), the child process will likely fail to bind the socket or
/// start the server — that's fine, we only care that it didn't spawn a swarm.
#[test]
#[allow(unsafe_code)]
fn test_daemon_start_does_not_fork_bomb() {
    // Build the binary first (assert_cmd does this lazily, but we need the
    // path upfront to grep for it in the process table).
    let bin_path = assert_cmd::cargo::cargo_bin!("ty-find");

    // Use a unique socket path so we don't interfere with a real daemon.
    // We achieve this by removing any existing socket so the "already running"
    // check doesn't short-circuit.
    //
    // SAFETY: `libc::getuid()` is a simple syscall that returns the real
    // user ID. It has no preconditions and cannot cause UB.
    let socket_path = format!("/tmp/ty-find-{}.sock", unsafe { libc::getuid() });
    let _ = std::fs::remove_file(&socket_path);

    // Spawn the CLI command (not --foreground, so it goes through the
    // spawn_background path).
    let mut child = Command::new(bin_path.as_os_str())
        .arg("daemon")
        .arg("start")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn ty-find");

    // Give it a moment to potentially fork-bomb.
    std::thread::sleep(Duration::from_secs(2));

    // Count ty-find processes.  We look for the binary name in the process
    // list via `pgrep`.
    let count = count_ty_find_processes(bin_path);

    // Clean up: kill the parent (and any children) we started.
    let _ = child.kill();
    let _ = child.wait();

    // Also try to clean up any leftover children via pkill (best-effort).
    let _ = Command::new("pkill").arg("-f").arg(bin_path.to_string_lossy().as_ref()).output();

    // Clean up stale socket.
    let _ = std::fs::remove_file(&socket_path);

    // A correct implementation should have at most 2 processes (parent + one
    // background child).  Allow a small margin but anything above 5 is a
    // clear fork bomb.
    assert!(
        count <= 5,
        "Detected {count} ty-find processes — likely a fork bomb! \
         Expected at most 2 (parent + 1 background child).",
    );
}

/// Test that a daemon-dependent command auto-starts the daemon when it is not
/// already running.
///
/// Strategy:
/// 1. Ensure no daemon is running (stop + remove socket).
/// 2. Run a `hover` command which goes through `ensure_daemon_running()`.
/// 3. Verify that the daemon socket was created (proving auto-start fired).
/// 4. Clean up.
///
/// The hover request itself may fail (ty LSP may not be installed), but the
/// daemon server should have been spawned and its socket should exist.
#[test]
#[allow(unsafe_code)]
fn test_daemon_auto_start_on_first_request() {
    let bin_path = assert_cmd::cargo::cargo_bin!("ty-find");
    // SAFETY: `libc::getuid()` is a simple syscall with no preconditions.
    let socket_path = format!("/tmp/ty-find-{}.sock", unsafe { libc::getuid() });

    // --- setup: make sure no daemon is running ---
    let _ = Command::new(bin_path.as_os_str()).arg("daemon").arg("stop").output();
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::fs::remove_file(&socket_path);

    // Sanity-check: socket must not exist.
    assert!(!Path::new(&socket_path).exists(), "Socket still present after cleanup");

    // --- act: run a daemon-dependent command (hover) ---
    // Create a minimal Python file so the CLI doesn't bail on missing file before
    // reaching the daemon auto-start path.
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let test_file = temp_dir.path().join("test.py");
    std::fs::write(&test_file, "x = 1\n").expect("failed to write test file");

    // The hover command will call ensure_daemon_running() then try to use the
    // daemon.  The LSP request may fail, but we only care that the daemon was
    // spawned.
    let _output = Command::new(bin_path.as_os_str())
        .arg("hover")
        .arg(&test_file)
        .arg("-l")
        .arg("1")
        .arg("-c")
        .arg("1")
        .output()
        .expect("failed to run hover command");

    // Give the daemon a moment to fully bind the socket (it may already be
    // bound because ensure_daemon_running polls for it, but be safe).
    std::thread::sleep(Duration::from_millis(500));

    // --- assert: daemon socket should exist ---
    let socket_exists = Path::new(&socket_path).exists();

    // Double-check via daemon status.
    let status = Command::new(bin_path.as_os_str())
        .arg("daemon")
        .arg("status")
        .output()
        .expect("failed to run daemon status");
    let status_stdout = String::from_utf8_lossy(&status.stdout);

    // --- cleanup ---
    let _ = Command::new(bin_path.as_os_str()).arg("daemon").arg("stop").output();
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::fs::remove_file(&socket_path);

    assert!(
        socket_exists || status_stdout.contains("running"),
        "Daemon was not auto-started. Socket exists: {socket_exists}, status output: {status_stdout}",
    );
}

/// Count how many processes match the ty-find binary path.
fn count_ty_find_processes(bin_path: &std::path::Path) -> usize {
    // Use `pgrep -f` to match the full command line against the binary path.
    let output = Command::new("pgrep").arg("-f").arg(bin_path.to_string_lossy().as_ref()).output();

    if let Ok(out) = output {
        // pgrep outputs one PID per line.
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout.lines().filter(|l| !l.trim().is_empty()).count()
    } else {
        // pgrep not available — fall back to ps + grep.
        let ps = Command::new("ps").arg("aux").output().expect("ps command failed");
        let ps_output = String::from_utf8_lossy(&ps.stdout);
        let bin_name =
            bin_path.file_name().expect("binary should have a filename").to_string_lossy();
        ps_output
            .lines()
            .filter(|line| line.contains(bin_name.as_ref()) && !line.contains("grep"))
            .count()
    }
}
